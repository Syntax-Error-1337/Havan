# Reference firmware — ESP-12E (ESP8266)

The smallest firmware that exercises the real protocol against the real broker: claim →
certificate → mutual TLS on 8883 → shadow → command → ack / typed refusal. It exists because
`sdk/c` builds the bytes, and a component nobody runs is a document.

**Status, precisely — on real hardware, not asserted:**

| Step | Result |
|---|---|
| Build | ✅ RAM 39.2 % (32 104/81 920), flash 41.8 % (436 579/1 044 464) |
| Flash | ✅ `Wrote 440400 bytes … Hash of data verified` |
| Boot via the OTA bootloader | ✅ `boot mode:(3,6)`, `load 0x3fff20b8` (eboot), flash 4 MB |
| Wi-Fi | ✅ `wifi: 192.168.0.208` (association is flaky; see below) |
| Claim over HTTP | ✅ `devices.state = claimed`, `fw_version = 1.0.0` |
| Device certificate | ✅ `cert_serial` set, `cert_not_after = 2027-11-14` |
| MQTT over mutual TLS | ❌ BearSSL rejects the handshake (evidence below) |

**The MQTT failure had two causes, and only the second one is on this board.**

The first was the platform's: `HAVEN_CA_DIR` defaulted to a path relative to the process's
working directory, so an API started from `backend/` signed device certificates with
`backend/.haven/ca` while the broker trusted `.haven/ca`. Every certificate the device received
was one the broker had never heard of, and the rejection arrives as a TLS alert that says nothing
about CAs. `make` now pins one CA for the API, the broker and `make broker-tls`, and the resolved
path is absolute and logged. That is the failure that made a working certificate look broken.

The second is real and is about memory: the handshake allocates ~20 KB out of ~36 KB free
(`heap 36792` before, `16592` after the failed attempt), so the exchange dies partway with no
BearSSL error code. 2 KB session buffers plus freeing the PEM strings after the objects are built
keeps the heap healthy, but the association also drops during the attempt, and the outcome is
unproven rather than fixed. The server-side half of this is proven independently: a certificate
issued for this device completes mutual TLS from the Mac against the same broker.

Wi-Fi associates and then drops (`status 7` / `status 6`, RSSI −34…−51, so not a signal
problem). `WiFi.begin()` aborts an association already in flight — retrying faster than the join
takes is a way of never joining — so the retry backoff is 20 s.

By the end of the session the board stopped associating at all, after roughly forty
flash-and-reboot cycles in a day, which is how an access point treats a MAC address it has decided
is misbehaving. **The first thing to try is therefore not firmware:** power-cycle the board, leave
it ten minutes, check the 2.4 GHz AP is WPA2-PSK rather than WPA3-only or PMF-required, and give
the MAC a DHCP reservation so the address stops moving.

### The constrained path exists now

8884 is network-facing and authenticates twice: the credential (device id + secret, checked by the
API through the broker's webhook) and the topic ACL (a device may only touch
`haven/<vendor>/<home>/<device>/…`). `secrets.ini.example` documents the two build flags that put
this firmware on it: `-D HAVEN_MQTT_PORT=8884 -D HAVEN_NO_CLIENT_CERT=1`. The username must be the
**device id** from the claim, not the serial — the broker looks the device up by id.

The board still does not associate with Wi-Fi, so this path is proven from the host
(`scripts/mqtt-probe.py`, a few hundred bytes of MQTT over TLS) and not yet from the board.

### The one test that separates board from broker

Point this firmware at a minimal `openssl s_server` on the development machine:

```bash
openssl s_server -accept 4433 -cert deploy/certs/server.crt -key deploy/certs/server.key -www -state
# then, in secrets.ini: -D HAVEN_MQTT_PORT=4433 -D HAVEN_NO_CLIENT_CERT=1
```

If the handshake completes there, the fleet's broker is at fault and the answer is listener
configuration. If it stalls the same way, the board is at fault and the answer is an ESP32-S3.
That test was set up but never produced a result, because the board could not keep a Wi-Fi
association long enough to run it — which is why it is written down here as the next step rather
than reported as a finding.

## Build and flash

```bash
python3 -m pip install --user platformio          # once; pio is then at ~/Library/Python/3.9/bin/pio
cp secrets.ini.example secrets.ini                # then fill in SSID, password, and the host
pio run -d sdk/esp8266                            # build
pio run -d sdk/esp8266 -t upload                  # flash   (--upload-port /dev/cu.usbserial-…)
pio device monitor -b 115200                      # logs
```

`secrets.ini` is gitignored and carries every value that must not be in the repository: the
Wi-Fi pair, this device's serial and secret, and the pairing ticket.

Two things about how PlatformIO reads it, both learned the hard way:

- `extra_configs` belongs in **`[platformio]`**, not in `[env:…]`; from an env section,
  `${secrets.build_flags}` fails with `No section: 'secrets'`.
- an extra config **cannot redefine a list option the main file already sets** — the main
  `build_flags` wins and the extra one is dropped silently. The main file must interpolate it:
  `${secrets.build_flags}` on its own line. Without that the firmware cheerfully joins
  `YOUR_SSID`, which is nine characters and was the only clue in the log.

`HAVEN_HOST` is the machine's **current** LAN address, because a device on the same Wi-Fi cannot
reach `127.0.0.1` and DHCP moves the address. Regenerate the broker certificate when it changes:
`make broker-tls`.

That certificate carries the LAN address as a **DNS** SAN as well as an IP SAN, because BearSSL
matches DNS names only and reports the mismatch as `Expected server name was not found in the
chain` — which sounds nothing like "your SAN is an IP address".

## What is on the board

| Datapoint | GPIO | What it shows |
|---|---|---|
| `1` power (bool) | `LED_BUILTIN` (GPIO2, active low) | a `set` turns the LED on/off |
| `2` brightness (int 0–100) | GPIO4, PWM 8-bit | a `set` changes PWM duty |

Two points is enough to demonstrate the checklist's interesting cases: an ack with the value
echoed, a type mismatch (`bad_type`), and a range refusal (`out_of_range`) that is **refused,
not clamped** — the app must be able to tell "I asked for 150" from "I got 100".

## What it does on first boot

1. Wi-Fi station, then NTP — **before** TLS, because an ESP8266 boots at 1970 and every chain
   check fails with a confusing "not yet valid".
2. No certificate on the filesystem → `POST /v1/provision/claim` with the serial, the secret and
   a pairing ticket from the app, then `POST /v1/provision/certificate`.
3. Stores `device.crt`, `device.key` and `ca.crt` in LittleFS and connects with mutual TLS.
4. Subscribes on CONNACK — the contract's rule about coming back deaf — publishes the retained
   shadow, and answers commands.

The ticket is single-use and expires in five minutes, so the flash that carries it has to be
fresh. After the first claim the device keeps its certificate and the ticket is irrelevant.

## What this board cannot do, and it is not a software problem

| Contract item | Why an ESP8266 fails it |
|---|---|
| §2 BLE provisioning (checklist 1–2) | **No Bluetooth radio exists** in the ESP8266. The radar cannot see this device at all |
| §1 "key storage MUST be encrypted" | No flash encryption, no eFuses, no secure boot. The key sits in LittleFS in the clear — a real certification failure, not a TODO |
| §4 TLS **1.3** | BearSSL stops at TLS 1.2 |
| Checklist 9, OTA rollback | 4 MB flash gives ~1 MB slots; there is no verified-boot mechanism to roll back to |

Everything above lives *outside* `haven.c`, so the ESP32‑S3 version is the same firmware with
BLE provisioning, an encrypted NVS partition and a bigger OTA layout added — not a rewrite. The
protocol work, the two GPIOs and the command handling are identical.

## Memory

Measured on this build: **RAM 39.1 %** (32 028 / 81 920 B), **flash 41.8 %** (436 247 / 1 044 464 B
per slot). Mutual TLS with an ECDSA client certificate is the tight part, and the numbers say
there is room — the certificates themselves are ~600–800 bytes each.
