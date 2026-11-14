// Reference firmware — ESP-12E (ESP8266), Arduino core.
//
// What this proves, against the real platform rather than a mock:
//   claim → certificate → mutual TLS on 8883 → shadow → command → ack / typed refusal.
//
// What it deliberately does not do, because the silicon cannot: BLE provisioning (no radio),
// encrypted key storage (no flash encryption on an ESP8266), TLS 1.3. Those are the three
// things an ESP32-S3 adds; nothing here has to be rewritten to get them, because the protocol
// work all lives in `haven.c` and this file only moves bytes and drives two GPIOs.

#include <Arduino.h>
#include <ArduinoJson.h>
#include <ESP8266WiFi.h>
#include <LittleFS.h>
#include <PubSubClient.h>
#include <WiFiClientSecure.h>

extern "C" {
#include "haven.h"
}

// ---- the two points this reference device pretends to have -------------------
// A bulb's shape, reduced to what an ESP-12E can actually show: a relay/LED for `1` and PWM
// brightness for `2`. The platform does not care what the hardware is; it cares that the
// datapoint ids match the product's thing model.
static const char *DP_POWER = "1";
static const char *DP_BRIGHTNESS = "2";
static const uint8_t PIN_POWER = LED_BUILTIN;
static const uint8_t PIN_BRIGHTNESS = 4;

// ---- configuration, from the build or the filesystem ------------------------
// The ticket is short-lived and comes from the app, so it is a compile-time flag for the first
// claim and then removed; the serial and secret are burned at manufacture (here: set in the
// build), and the certificate replaces the secret for every connection after the first.
#ifndef HAVEN_HOST
#define HAVEN_HOST "192.168.0.49"
#endif
#ifndef HAVEN_UI_PORT
#define HAVEN_UI_PORT 8080
#endif
#ifndef HAVEN_MQTT_PORT
#define HAVEN_MQTT_PORT 8883
#endif

// Credentials come from `secrets.ini` (untracked) through build flags. A Wi-Fi password in a
// repository is a password published, and a password in a chat message is the same thing.
#ifdef HAVEN_WIFI_SSID
static const char *WIFI_SSID = HAVEN_WIFI_SSID;
static const char *WIFI_PASS = HAVEN_WIFI_PASS;
#else
static const char *WIFI_SSID = "YOUR_SSID";
static const char *WIFI_PASS = "YOUR_PASSWORD";
#endif
#ifdef HAVEN_PRODUCT_KEY
static const char *PRODUCT_KEY = HAVEN_PRODUCT_KEY;
#else
static const char *PRODUCT_KEY = "hv_acme_bulb1";
#endif

// Not `SERIAL`: the ESP8266 core defines that as a macro.
#ifdef HAVEN_DEVICE_SERIAL
static const char *DEVICE_SERIAL = HAVEN_DEVICE_SERIAL;
#else
static const char *DEVICE_SERIAL = "SIM-0001";
#endif

#ifdef HAVEN_DEVICE_SECRET
static const char *SECRET = HAVEN_DEVICE_SECRET;
#else
static const char *SECRET = "replace-me-32-bytes";
#endif
static const char *FIRMWARE = "1.0.0";
#ifdef HAVEN_TICKET_VALUE
static const char *TICKET = HAVEN_TICKET_VALUE;
#else
static const char *TICKET = "";
#endif

static WiFiClientSecure tls;
static PubSubClient mqtt(tls);
static haven_link link;
static const char *LINK_PATH = "/link.json";

// Defined next to the certificate code further down; `provision()` runs before them and is the
// caller that saves the link.
static bool save_link();
static bool load_link();
// 128, not 96. One topic is `haven/` + three UUIDs + a lane: 6 + 36 + 1 + 36 + 1 + 36 + 4 = 120
// characters, and a 96-byte buffer truncated the device id mid-way. The truncated topic is a valid
// shape and an invalid topic, so the broker's ACL refused the *last will* — and MQTT requires
// rejecting the whole CONNECT when a will is not authorized, which arrives at the device as
// "return code 5, not authorized": the same answer a wrong password gets.
static char cmd_topic[128];
static char shadow_topic[128];
static char event_topic[128];
static char ack_topic[128];

static bool power_on = false;
static int brightness = 80;

// ---- helpers ---------------------------------------------------------------

static void apply_outputs() {
    digitalWrite(PIN_POWER, power_on ? HIGH : LOW);
    // The built-in LED on an ESP-12E is active low; the PWM point is a plain GPIO.
    analogWrite(PIN_BRIGHTNESS, map(brightness, 0, 100, 0, 255));
}

/// Write a PEM string to the filesystem, if it differs. Rewriting littlefs on every boot is
/// flash wear for no reason.
static bool store_pem(const char *path, const char *value) {
    if (LittleFS.exists(path)) {
        File existing = LittleFS.open(path, "r");
        String current = existing.readString();
        existing.close();
        if (current == value) return true;
    }
    File f = LittleFS.open(path, "w");
    if (!f) return false;
    f.print(value);
    f.close();
    return true;
}

static String load_pem(const char *path) {
    if (!LittleFS.exists(path)) return String();
    File f = LittleFS.open(path, "r");
    String value = f.readString();
    f.close();
    return value;
}

/// POST JSON and hand the body back. Returns the HTTP status.
static int http_post(const String &path, const String &body, String &out) {
    WiFiClient plain;
    if (!plain.connect(HAVEN_HOST, HAVEN_UI_PORT)) return -1;
    plain.print(String("POST ") + path + " HTTP/1.1\r\n");
    plain.print(String("Host: ") + HAVEN_HOST + "\r\n");
    plain.print("Content-Type: application/json\r\n");
    plain.print(String("Content-Length: ") + body.length() + "\r\n");
    plain.print("Connection: close\r\n\r\n");
    plain.print(body);

    // Headers first, then the body — a device on 80 KB of RAM must not buffer what it does not
    // need, so the status line is read and the rest is skipped by hand.
    unsigned long deadline = millis() + 8000;
    int status = -1;
    out = "";
    bool in_body = false;
    while (plain.connected() || plain.available()) {
        if (millis() > deadline) break;
        while (plain.available()) {
            String line = plain.readStringUntil('\n');
            if (!in_body) {
                if (line.startsWith("HTTP/")) status = line.substring(9, 12).toInt();
                if (line == "\r" || line.length() == 0) in_body = true;
            } else {
                out += line;
            }
        }
        if (!plain.available()) delay(1);
        if (in_body && !plain.available()) break;
    }
    plain.stop();
    return status;
}

/// Claim, and ask for the certificate in the same breath.
///
/// The certificate is what every later connection uses; the secret is spent exactly here. It is
/// stored on littlefs in the clear, which is precisely the §1 certification failure an ESP8266
/// cannot avoid — said out loud rather than papered over.
static bool provision() {
    haven_claim_request request = {PRODUCT_KEY, DEVICE_SERIAL, SECRET, TICKET, FIRMWARE};
    char body[512];
    if (haven_claim_body(&request, body, sizeof body) != HAVEN_OK) {
        Serial.println("claim body failed");
        return false;
    }

    String response;
    int status = http_post("/v1/provision/claim", body, response);
    Serial.printf("claim: %d\n", status);
    if (status != 201) return false;

    static StaticJsonDocument<512> doc;
    doc.clear();
    if (deserializeJson(doc, response) != DeserializationError::Ok) return false;

    const char *vendor = doc["vendor_id"] | "";
    const char *home = doc["home_id"] | "";
    const char *device = doc["device_id"] | "";
    strncpy(link.vendor_id, vendor, sizeof link.vendor_id - 1);
    strncpy(link.home_id, home, sizeof link.home_id - 1);
    strncpy(link.device_id, device, sizeof link.device_id - 1);

    // The certificate request does not need the ticket: the secret is the credential.
    static StaticJsonDocument<256> req;
    req.clear();
    req["pk"] = PRODUCT_KEY;
    req["sn"] = DEVICE_SERIAL;
    req["secret"] = SECRET;
    char cert_body[256];
    serializeJson(req, cert_body, sizeof cert_body);

    response = "";
    status = http_post("/v1/provision/certificate", cert_body, response);
    Serial.printf("certificate: %d\n", status);
    if (status != 201) return false;

    static StaticJsonDocument<4096> certs;
    certs.clear();
    if (deserializeJson(certs, response) != DeserializationError::Ok) return false;

    // Saved before the certificates: the identity is what makes them usable, and a device that
    // keeps a certificate but forgets its ids is a device that cannot publish.
    save_link();

    return store_pem("/device.crt", certs["certificate_pem"] | "") &&
           store_pem("/device.key", certs["private_key_pem"] | "") &&
           store_pem("/ca.crt", certs["ca_pem"] | "");
}

// BearSSL copies the DER into these objects and deletes copy-assignment, so they are built
// once and owned here. They have to outlive every connection that points at them.
static BearSSL::X509List *certList = nullptr;
static BearSSL::X509List *caList = nullptr;
static BearSSL::PrivateKey *privateKey = nullptr;

static void free_tls() {
    delete certList;
    delete caList;
    delete privateKey;
    certList = nullptr;
    caList = nullptr;
    privateKey = nullptr;
}

/// Point the TLS client at the stored material.
/// The claim's result, kept on flash next to the certificates.
///
/// A device reboots constantly, and the certificate alone does not say which topics it may use:
/// vendor, home and device id come from the claim response. Without this the first reboot
/// reconnected as an anonymous station with empty ids, and the broker's answer — MQTT return code
/// 5, "not authorized" — was the same one a wrong password gets.
static bool save_link() {
    File f = LittleFS.open(LINK_PATH, "w");
    if (!f) return false;
    f.printf("{\"vendor_id\":\"%s\",\"home_id\":\"%s\",\"device_id\":\"%s\"}",
             link.vendor_id, link.home_id, link.device_id);
    f.close();
    Serial.println("link: saved");
    return true;
}

static bool load_link() {
    File f = LittleFS.open(LINK_PATH, "r");
    if (!f) return false;
    static StaticJsonDocument<512> doc;
    doc.clear();
    DeserializationError err = deserializeJson(doc, f);
    f.close();
    if (err) return false;
    strncpy(link.vendor_id, doc["vendor_id"] | "", sizeof link.vendor_id - 1);
    strncpy(link.home_id, doc["home_id"] | "", sizeof link.home_id - 1);
    strncpy(link.device_id, doc["device_id"] | "", sizeof link.device_id - 1);
    return link.vendor_id[0] && link.home_id[0] && link.device_id[0];
}

static bool load_tls() {
    String cert = load_pem("/device.crt");
    String key = load_pem("/device.key");
    String ca = load_pem("/ca.crt");
    if (!cert.length() || !key.length() || !ca.length()) return false;
    // A PEM truncated on the way to LittleFS and a certificate the broker rejects look identical
    // from `mqtt: failed state=-2`, so the sizes are logged: a leaf certificate is ~800 bytes and
    // an EC private key ~240, and a short one is a storage bug rather than a TLS one.
    Serial.printf("tls: crt=%u key=%u ca=%u heap=%u\n", cert.length(), key.length(), ca.length(),
                  ESP.getFreeHeap());

    free_tls();
    caList = new BearSSL::X509List(ca.c_str());
    certList = new BearSSL::X509List(cert.c_str());
    privateKey = new BearSSL::PrivateKey(key.c_str());

    // The three objects above copied what they need out of the strings, and every byte matters:
    // the handshake wants ~20 KB and a fresh ESP8266 firmware has ~36 KB. `String` keeps its
    // capacity on clear(), so assigning an empty one actually returns the memory.
    cert = String();
    key = String();
    ca = String();
    Serial.printf("tls: loaded, heap %u\n", ESP.getFreeHeap());

    // 4 KB in / 1 KB out rather than the 16 KB default. This decides whether mutual TLS fits at
    // all: the buffer bounds the largest TLS record the client accepts, and a handshake record
    // bigger than it does not fail — it *hangs*, which is what a device looks like when the Wi-Fi
    // drops mid-connect. 2 KB stalled; 4 KB still leaves ~30 KB for the certificates.
    tls.setBufferSizes(4096, 1024);
    // A handshake that never completes must not hold the link forever: the watchdog resets the
    // board and the log looks like a Wi-Fi fault rather than a stalled exchange.
    tls.setTimeout(10000);
    tls.setTrustAnchors(caList);
#ifdef HAVEN_NO_CLIENT_CERT
    // The constrained listener authenticates at the MQTT layer with this device's own serial and
    // secret, because a client certificate plus its chain does not fit in an ESP8266's heap: the
    // mutual handshake allocates ~20 KB of ~36 KB and the exchange stalls partway. Ranked against
    // a device that cannot connect at all, a per-device credential over TLS is the better trade —
    // and it is exactly why 8883 (certificates) stays the default for devices that can afford it.
    Serial.println("tls: MQTT credentials, no client certificate");
#else
    // The device key is ECDSA P-256 (that is what the platform issues); BearSSL needs to be told
    // both that it is EC and what the chain is allowed to do with it.
    tls.setClientECCert(certList, privateKey, BR_KEYTYPE_KEYX | BR_KEYTYPE_SIGN, BR_KEYTYPE_EC);
#endif
    return true;
}

/// Publish the retained shadow, so a device that reboots is current immediately.
static void publish_shadow() {
    haven_dp pts[2] = {
        {DP_POWER, HAVEN_DP_BOOL, {.b = power_on}},
        {DP_BRIGHTNESS, HAVEN_DP_INT, {.i = brightness}},
    };
    char payload[HAVEN_ENVELOPE_MAX];
    if (haven_shadow(pts, 2, (int64_t)time(nullptr), true, FIRMWARE, payload, sizeof payload) !=
        HAVEN_OK) {
        Serial.println("shadow build failed");
        return;
    }
    mqtt.publish(shadow_topic, payload, true);
}

/// Report the points on the event lane, which is what tells the platform the device exists.
///
/// The shadow is retained state for whoever reads it next; the event is the report that makes a
/// device *online* and produces telemetry. A firmware that publishes only the shadow is a device
/// the broker knows about and the platform does not — it sits there connected, looking offline.
static void publish_event() {
    haven_dp pts[2] = {
        {DP_POWER, HAVEN_DP_BOOL, {.b = power_on}},
        {DP_BRIGHTNESS, HAVEN_DP_INT, {.i = brightness}},
    };
    char payload[HAVEN_ENVELOPE_MAX];
    if (haven_state_event(pts, 2, (int64_t)time(nullptr), payload, sizeof payload) != HAVEN_OK) {
        Serial.println("event build failed");
        return;
    }
    mqtt.publish(event_topic, payload, false);
}

/// The command lane: apply, or refuse with a reason the app can show.
static void on_command(char *payload, unsigned int length) {
    StaticJsonDocument<512> cmd;
    if (deserializeJson(cmd, payload, length) != DeserializationError::Ok) return;

    const char *id = cmd["id"] | "";
    const char *dp = cmd["dp"] | "";
    if (!strlen(id) || !strlen(dp)) return;

    int64_t now = (int64_t)time(nullptr);
    char out[HAVEN_ENVELOPE_MAX];
    haven_dp value;

    if (strcmp(dp, DP_POWER) == 0) {
        // A bool point accepts only a bool. A device that coerces "true" or 1 into a state has
        // invented behaviour its product never declared.
        if (!cmd["value"].is<bool>()) {
            haven_refusal(id, now, haven_refusal_bad_type(), "power is a bool", out, sizeof out);
            mqtt.publish(ack_topic, out);
            return;
        }
        power_on = cmd["value"].as<bool>();
        apply_outputs();
        value = {DP_POWER, HAVEN_DP_BOOL, {.b = power_on}};
    } else if (strcmp(dp, DP_BRIGHTNESS) == 0) {
        if (!cmd["value"].is<int>()) {
            haven_refusal(id, now, haven_refusal_bad_type(), "brightness is an integer", out,
                          sizeof out);
            mqtt.publish(ack_topic, out);
            return;
        }
        int next = cmd["value"].as<int>();
        if (next < 0 || next > 100) {
            // Refused, not clamped: the app must be able to tell "I asked for 150" from "I got
            // 100", and a silent clamp makes that impossible.
            haven_refusal(id, now, haven_refusal_out_of_range(), "brightness must be 0-100", out,
                          sizeof out);
            mqtt.publish(ack_topic, out);
            return;
        }
        brightness = next;
        apply_outputs();
        value = {DP_BRIGHTNESS, HAVEN_DP_INT, {.i = brightness}};
    } else {
        haven_refusal(id, now, haven_refusal_unknown_dp(), "this device has no such datapoint",
                      out, sizeof out);
        mqtt.publish(ack_topic, out);
        return;
    }

    if (haven_ack(id, dp, &value, now, out, sizeof out) == HAVEN_OK) {
        mqtt.publish(ack_topic, out);
    }
    // The shadow follows the ack: an app that reloads the screen sees what the device reports,
    // not what it was asked for.
    publish_shadow();
}

static void on_message(char *topic, byte *payload, unsigned int length) {
    if (cmd_topic[0] && strcmp(topic, cmd_topic) != 0) return;

    // Copy into a NUL-terminated buffer: the payload is not terminated and JSON parsing on
    // whatever happens to follow it in RAM is a crash waiting for a field to be short.
    static char buffer[HAVEN_ENVELOPE_MAX];
    if (length >= sizeof buffer) length = sizeof buffer - 1;
    memcpy(buffer, payload, length);
    buffer[length] = '\0';

    on_command(buffer, length);
}

static bool mqtt_connect() {
    // A plain TCP probe first: it separates "the network cannot reach the broker" from "the TLS
    // handshake was rejected", which look identical from PubSubClient's -2.
    WiFiClient probe;
    bool tcp = probe.connect(HAVEN_HOST, HAVEN_MQTT_PORT);
    probe.stop();
    Serial.printf("mqtt: connecting to %s:%d (tcp %s, heap %u)\n", HAVEN_HOST, HAVEN_MQTT_PORT,
                  tcp ? "ok" : "failed", ESP.getFreeHeap());
    if (!tcp) return false;
    // A clean session with an explicit resubscribe on CONNACK is the contract's rule: a
    // persistent session that the broker drops leaves the device deaf and nothing says so.
#ifdef HAVEN_NO_CLIENT_CERT
    // The username is the **device id** the platform minted at claim time, not the serial: the
    // broker's hook looks the device up by id, and a serial is only unique within a product. The
    // password is the secret the device claimed with, and the ACL keeps it inside its own topics.
    const char *user = link.device_id;
    const char *pass = SECRET;
#else
    const char *user = nullptr;
    const char *pass = nullptr;
#endif
    // The last will is a real envelope with `online: false`, on the shadow lane. A literal
    // "offline" string on the event lane is what the worker logged as
    // `evt rejected ... expected value at line 1 column 1` — a will is parsed like any other
    // message, so it has to be one.
    static char will_payload[HAVEN_ENVELOPE_MAX];
    {
        haven_dp pts[2] = {
            {DP_POWER, HAVEN_DP_BOOL, {.b = power_on}},
            {DP_BRIGHTNESS, HAVEN_DP_INT, {.i = brightness}},
        };
        if (haven_shadow(pts, 2, (int64_t)time(nullptr), false, FIRMWARE, will_payload,
                         sizeof will_payload) != HAVEN_OK) {
            will_payload[0] = '\0';
        }
    }
    if (!mqtt.connect(link.device_id, user, pass, shadow_topic[0] ? shadow_topic : event_topic, 1,
                      true, will_payload)) {
        // The MQTT state alone cannot tell a TLS failure from a broker refusal, and the TLS
        // failure is the one a mutual-auth device actually hits. BearSSL's error code and the
        // free heap distinguish "the handshake was rejected" from "the handshake ran out of
        // memory", which are different afternoons.
        char ssl_error[64] = {0};
        tls.getLastSSLError(ssl_error, sizeof ssl_error);
        Serial.printf("mqtt: failed state=%d ssl=%s heap=%u\n", mqtt.state(), ssl_error,
                      ESP.getFreeHeap());
        return false;
    }
    // The command lane. Subscribing to `event_topic` was the bug that made a device publish
    // happily and never receive a single command: it was listening to its own reports.
    mqtt.subscribe(cmd_topic, 1);
    publish_shadow();
    // The report is what makes this device *online* on the platform; the shadow alone is state
    // nobody is watching.
    publish_event();
    return true;
}

static bool tls_ready = false;
static bool clock_set = false;

/// The SDK's disconnect reason, which `WiFi.status()` cannot express.
///
/// Status 6 is "wrong password" for a wrong password, for a four-way handshake that timed out, and
/// for an AP that deauthenticated the station — three problems with one message, and the reason
/// code is the only thing that tells them apart. 15 = handshake timeout (key or PMF mismatch),
/// 8 = the AP sent a deauth, 2 = auth expired, 201 = no AP found, 203 = association refused.
static const char *wifi_reason(uint8_t r) {
    switch (r) {
        case 2: return "AUTH_EXPIRE";
        case 6: return "NOT_AUTHED";
        case 7: return "NOT_ASSOCED";
        case 8: return "ASSOC_LEAVE (AP deauthenticated us)";
        case 15: return "4WAY_HANDSHAKE_TIMEOUT (key, TKIP or PMF mismatch)";
        case 200: return "BEACON_TIMEOUT";
        case 201: return "NO_AP_FOUND";
        case 202: return "AUTH_FAIL";
        case 203: return "ASSOC_FAIL";
        case 204: return "HANDSHAKE_TIMEOUT";
        default: return "other";
    }
}

static void on_station_disconnected(const WiFiEventStationModeDisconnected &e) {
    Serial.printf("wifi: disconnected reason=%u (%s)\n", e.reason, wifi_reason(e.reason));
}

static void on_station_got_ip(const WiFiEventStationModeGotIP &e) {
    Serial.printf("wifi: got ip %s\n", e.ip.toString().c_str());
}

/// Bring up the station, once. Returns immediately; `loop()` waits for the result.
static void start_wifi() {
    WiFi.persistent(false);
    WiFi.mode(WIFI_STA);
    // Modem sleep parks the radio between beacons; a TLS handshake that lands in a sleep window
    // stalls and the association drops. This device is mains-powered and sends little, so the
    // power saving buys nothing and costs connections.
    WiFi.setSleepMode(WIFI_NONE_SLEEP);
#ifdef HAVEN_WIFI_PHY_11G
    // 802.11g only. An access point that offers HT/40 MHz alongside a WPA/WPA2 mixed cipher set is
    // where an ESP8266's association tends to die, and dropping to 11g costs throughput this
    // device does not need. A workaround, not the fix: the SSID should be WPA2-AES only.
    WiFi.setPhyMode(WIFI_PHY_MODE_11G);
#endif
    WiFi.begin(WIFI_SSID, WIFI_PASS);
    Serial.printf("wifi: joining %u-char ssid\n", (unsigned)strlen(WIFI_SSID));
}

void setup() {
    Serial.begin(115200);
    delay(200);
    Serial.println("\nhaven device: esp-12e");

    pinMode(PIN_POWER, OUTPUT);
    pinMode(PIN_BRIGHTNESS, OUTPUT);
    // 8-bit PWM: the same range the brightness point declares (0-100) maps onto it directly,
    // and `PWMRANGE` no longer exists as a constant in core 3.x.
    analogWriteRange(255);
    apply_outputs();

    LittleFS.begin();

    // Registered before the first `begin()`: a failure that happens during association is exactly
    // the one worth logging, and a listener added afterwards misses it.
    WiFi.onStationModeDisconnected(on_station_disconnected);
    WiFi.onStationModeGotIP(on_station_got_ip);

    start_wifi();

    mqtt.setServer(HAVEN_HOST, HAVEN_MQTT_PORT);
    // PubSubClient's default keepalive is 15 seconds, which is *shorter* than this loop's own
    // work: a heartbeat that builds and publishes a shadow, plus a TLS handshake when it
    // reconnects, is enough to miss a keepalive. The broker then publishes the will
    // (`online: false`) and drops the session mid-publish — which presented as a device that
    // applied a command, acked it, and immediately reported the old value, every ten seconds.
    mqtt.setKeepAlive(90);
    mqtt.setCallback(on_message);
    // Room for a command envelope and the shadow that follows it.
    mqtt.setBufferSize(HAVEN_ENVELOPE_MAX);

    haven_topic_event(&link, event_topic, sizeof event_topic);
    haven_topic_ack(&link, ack_topic, sizeof ack_topic);
}

/// Wi-Fi, clock, certificate, broker — in that order, retried rather than fatal.
///
/// The first version returned from `setup()` when Wi-Fi failed and then kept dialling the
/// broker from `loop()` with no network, which looked like an MQTT problem for an hour.
static void advance() {
    if (WiFi.status() != WL_CONNECTED) {
        static unsigned long last = 0;
        static int last_status = -1;
        int status = (int)WiFi.status();
        if (status != last_status) {
            last_status = status;
            // The code distinguishes "no such network" (1) from "wrong password" (6) from
            // "connecting" (7) — the difference between a typo and a 5 GHz-only router. The
            // reason behind a failure comes from the event callback above.
            Serial.printf("wifi: status %d, rssi %d\n", status, WiFi.RSSI());
        }
        // Twenty seconds, not five: `WiFi.begin()` aborts an association already in flight, so
        // retrying faster than the join takes is a way of never joining at all. This is the bug
        // that made the board associate, drop, and repeat.
        if (millis() - last > 20000) {
            last = millis();
            start_wifi();
        }
        return;
    }

    if (!clock_set) {
        // Trust nothing until the clock is set: certificate validity is checked against it, and
        // an ESP8266 boots at 1970 — every chain check then fails with a confusing "not yet
        // valid" rather than an obvious "your clock is wrong".
        configTime(0, 0, "pool.ntp.org");
        if (time(nullptr) < 1600000000) return;
        clock_set = true;
        Serial.printf("wifi: %s, clock %lu\n", WiFi.localIP().toString().c_str(),
                      (unsigned long)time(nullptr));
    }

    if (!tls_ready) {
        if (!link.device_id[0] && !load_link()) {
            Serial.println("no link on the device; claiming");
            if (!provision() || !load_tls()) {
                Serial.println("provisioning failed");
                return;
            }
        }
        if (!load_tls()) {
            Serial.println("no certificate on the device; claiming");
            if (!provision() || !load_tls()) {
                Serial.println("provisioning failed");
                return;
            }
        }
        haven_topic_event(&link, event_topic, sizeof event_topic);
        haven_topic_ack(&link, ack_topic, sizeof ack_topic);
        haven_topic_shadow(&link, shadow_topic, sizeof shadow_topic);
    haven_topic_cmd(&link, cmd_topic, sizeof cmd_topic);
        tls_ready = true;
    }

    mqtt_connect();
}

void loop() {
    static unsigned long last_heartbeat = 0;
    static unsigned long last_attempt = 0;

    if (!tls_ready || !mqtt.connected()) {
        if (millis() - last_attempt > 5000) {
            last_attempt = millis();
            advance();
        }
        // Still pump MQTT when it is connected-but-flagged; harmless when it is not.
        mqtt.loop();
        return;
    }

    if (!mqtt.connected()) {
        if (millis() - last_attempt > 5000) {
            last_attempt = millis();
            mqtt_connect();
        }
        return;
    }

    mqtt.loop();

    // A shadow that stops updating is indistinguishable from a device that is alive but idle, so
    // a heartbeat reports on a cadence as well as on change.
    if (millis() - last_heartbeat > 30000) {
        last_heartbeat = millis();
        publish_shadow();
        publish_event();
    }
}
