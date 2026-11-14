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

**The MQTT failure, as an experiment rather than a guess.** With
`require_certificate=on` the device logs `mqtt: failed state=-2`; with it `off` the failure
disappears without the firmware changing. A plain TCP probe to the same port succeeds from the
same board (`tcp ok, heap 36352`), and `openssl s_client` from the Mac completes the mutual
handshake with the same CA. So the network, the port and the server certificate are fine and the
**client-certificate path on BearSSL is what fails**. The next thing to examine is
`setClientECCert` against this core's BearSSL build; the ESP32-S3 path does not have this
problem, which is one more reason the certification board matters.

Wi-Fi associates and then drops (`status 7` / `status 6`, RSSI −43…−51, so not a signal
problem). `WiFi.begin()` aborts an association already in flight — retrying faster than the join
takes is a way of never joining — so the retry backoff is 20 s, and the flapping is now a router
question rather than a firmware one.

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
