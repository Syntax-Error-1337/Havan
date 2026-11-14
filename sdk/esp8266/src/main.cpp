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
static char shadow_topic[96];
static char event_topic[96];
static char ack_topic[96];

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

    StaticJsonDocument<512> doc;
    if (deserializeJson(doc, response) != DeserializationError::Ok) return false;

    const char *vendor = doc["vendor_id"] | "";
    const char *home = doc["home_id"] | "";
    const char *device = doc["device_id"] | "";
    strncpy(link.vendor_id, vendor, sizeof link.vendor_id - 1);
    strncpy(link.home_id, home, sizeof link.home_id - 1);
    strncpy(link.device_id, device, sizeof link.device_id - 1);

    // The certificate request does not need the ticket: the secret is the credential.
    StaticJsonDocument<256> req;
    req["pk"] = PRODUCT_KEY;
    req["sn"] = DEVICE_SERIAL;
    req["secret"] = SECRET;
    char cert_body[256];
    serializeJson(req, cert_body, sizeof cert_body);

    response = "";
    status = http_post("/v1/provision/certificate", cert_body, response);
    Serial.printf("certificate: %d\n", status);
    if (status != 201) return false;

    StaticJsonDocument<4096> certs;
    if (deserializeJson(certs, response) != DeserializationError::Ok) return false;

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
    // Copy into a NUL-terminated buffer: the payload is not terminated and JSON parsing on
    // whatever happens to follow it in RAM is a crash waiting for a field to be short.
    static char buffer[HAVEN_ENVELOPE_MAX];
    if (length >= sizeof buffer) length = sizeof buffer - 1;
    memcpy(buffer, payload, length);
    buffer[length] = '\0';

    if (strcmp(topic, event_topic) == 0 || strcmp(topic, ack_topic) == 0) {
        on_command(buffer, length);
    }
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
    // Username is the serial the platform issued the device; the password is its secret. The
    // broker's ACLs keep it inside its own topics.
    const char *user = DEVICE_SERIAL;
    const char *pass = SECRET;
#else
    const char *user = nullptr;
    const char *pass = nullptr;
#endif
    if (!mqtt.connect(link.device_id, user, pass, event_topic, 1, false, "offline")) {
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
    mqtt.subscribe(event_topic, 1);
    publish_shadow();
    return true;
}

static bool tls_ready = false;
static bool clock_set = false;

/// Bring up the station, once. Returns immediately; `loop()` waits for the result.
static void start_wifi() {
    WiFi.persistent(false);
    WiFi.mode(WIFI_STA);
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

    start_wifi();

    mqtt.setServer(HAVEN_HOST, HAVEN_MQTT_PORT);
    mqtt.setCallback(on_message);
    // Room for a command envelope and the shadow that follows it.
    mqtt.setBufferSize(HAVEN_ENVELOPE_MAX);

    snprintf(event_topic, sizeof event_topic, "haven/%s/%s/%s/evt", link.vendor_id, link.home_id,
             link.device_id);
    snprintf(ack_topic, sizeof ack_topic, "haven/%s/%s/%s/ack", link.vendor_id, link.home_id,
             link.device_id);
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
            // "connecting" (7) — the difference between a typo and a 5 GHz-only router.
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
        if (!load_tls()) {
            Serial.println("no certificate on the device; claiming");
            if (!provision() || !load_tls()) {
                Serial.println("provisioning failed");
                return;
            }
        }
        snprintf(event_topic, sizeof event_topic, "haven/%s/%s/%s/evt", link.vendor_id,
                 link.home_id, link.device_id);
        snprintf(ack_topic, sizeof ack_topic, "haven/%s/%s/%s/ack", link.vendor_id,
                 link.home_id, link.device_id);
        snprintf(shadow_topic, sizeof shadow_topic, "haven/%s/%s/%s/shadow", link.vendor_id,
                 link.home_id, link.device_id);
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
    }
}
