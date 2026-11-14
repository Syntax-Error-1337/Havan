# Reference firmware — ESP-12E (ESP8266)

The smallest firmware that exercises the real protocol against the real broker: claim →
certificate → mutual TLS on 8883 → shadow → command → ack / typed refusal. It exists because
`sdk/c` builds the bytes, and a component nobody runs is a document.

**Status, precisely:** builds, flashes and boots on the board in this repository's dev setup.
The radio-dependent half (Wi-Fi, TLS handshake, claim) has not been run yet — it needs the
Wi-Fi credentials in `secrets.ini`, which is where they belong rather than in this file.

## Build and flash

```bash
python3 -m pip install --user platformio          # once; pio is then at ~/Library/Python/3.9/bin/pio
cp secrets.ini.example secrets.ini                # then fill in SSID, password, and the host
pio run -d sdk/esp8266                            # build
pio run -d sdk/esp8266 -t upload                  # flash   (--upload-port /dev/cu.usbserial-…)
pio device monitor -b 115200                      # logs
```

`secrets.ini` is gitignored. The firmware compiles the placeholders from `main.cpp` when it is
missing, which fails at "wifi: failed" — the correct failure.

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
