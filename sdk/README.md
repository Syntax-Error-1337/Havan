# The Haven device SDK

Two implementations of one contract:

| | What it is | Status |
|---|---|---|
| `backend/crates/haven-sdk` | Rust reference implementation — topics, envelopes, reconnect, acks, OTA progress | **done, tested** |
| `c/` | C component for ESP-IDF / FreeRTOS (the shape a vendor flashes) | **not built** |

The Rust crate is not a wrapper for convenience: it is the behaviour the C component
must reproduce, written where it can be tested on a laptop. Everything a device can get
wrong silently lives in it, and the C component is correct when it matches.

## What the crate guarantees

- **Topics are built, never formatted by hand.** `Link` derives all five from
  `(vendor, home, device)`; a typo in a topic string is a device that connects,
  subscribes and never hears a command.
- **Subscriptions survive reconnects.** It subscribes on *every* `ConnAck`. With a clean
  session the broker forgets subscriptions, so a device that reconnects without this is
  online, healthy and deaf — the bug this SDK exists to prevent.
- **A command either applies or is refused in words.** `PointSet::apply` refuses an
  unknown datapoint and a type change (`unknown_dp`, `bad_type`), and the refusal goes
  back as an ack the app can show.
- **The shadow always carries the firmware version.** Without it "up to date" is
  unreachable for the customer.
- **OTA reports are one per step** (`downloading`, `verified`, `rebooting`, `success`),
  so a rollout table shows progress instead of a jump.
- **A broker blip is an event, not an exit.** `next()` never returns `None`; a device
  that exits is a device a customer power-cycles.

## Usage

```rust
let claimed = haven_sdk::claim("http://127.0.0.1:8080", &request).await?;
let link = haven_sdk::Link::from_claim(claimed.vendor_id, claimed.home_id, claimed.device_id);

let mut points = haven_sdk::PointSet::from_pairs([("1", DpValue::Bool(false))]);
let will = haven_sdk::DeviceClient::offline_will(&link, "1.0.0");
let mut client = haven_sdk::DeviceClient::connect(link, "mqtt://127.0.0.1:1883", "dev-1", Some(will)).await?;

loop {
    match client.next().await {
        haven_sdk::DeviceEvent::Command(cmd) => {
            let ts = kernel::now_s();
            match points.apply(&cmd) {
                Ok(()) => client.publish_ack(&haven_sdk::acknowledgement(&cmd, &points, ts)).await?,
                Err((code, msg)) => client.publish_ack(&haven_sdk::refusal(&cmd, ts, &code, &msg)).await?,
            }
        }
        haven_sdk::DeviceEvent::OtaOffer(offer) => { /* download, verify, report */ }
        _ => {}
    }
}
```

`backend/crates/simulator` is a complete device built on this API in ~130 lines — claim,
a product's datapoints, and a reaction to each event. That is the size a vendor's
application logic should be, because everything else is the SDK's job.

## What is not here

- **mTLS enrolment and the certification checklist** (`haven-cert`). The contract in
  `docs/03-SDK-CONTRACT.md` describes certificate-authenticated devices; the Rust crate
  today authenticates the way the simulator does, by claim + broker session.
- **The C component.** Needs a board and a toolchain, and a component nobody compiles is
  not a component.
- **The reference hardware.** The checklists in `docs/03-SDK-CONTRACT.md` are prose until
  there is an ESP32-S3 on a desk.

## The C component

`sdk/c` is the same surface in C, for firmware: topics built not formatted, envelopes for
state, shadow, ack, refusal and OTA progress, and the claim body. No transport — an ESP32 uses
`esp_mqtt_client` and `esp_http_client`, a gateway uses libmosquitto and curl, and baking one
in would make this a framework rather than a reference.

```bash
make sdk           # builds and runs 45 host checks with clang
```

It is transport-free C11 with no platform headers, and `sdk/c/CMakeLists.txt` registers it as
an ESP-IDF component, so dropping the directory into `components/` is the whole integration.

**Verified against the real server, not just against itself:** the shadow `emit.c` prints was
published to the broker and the API recorded `dp 1 = true`, `dp 2 = 80`, `fw_version = 1.0.0`,
`online = true` — so the C bytes and the Rust ingest agree.

**Not verified:** a flash on hardware. The certification checklist in
`docs/03-SDK-CONTRACT.md` stays prose until an ESP32 is on a desk.
