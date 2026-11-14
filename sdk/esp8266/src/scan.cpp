// A diagnostic, not a product: what this board can see, and why it will not join.
//
// The firmware prints "wifi: status 6" and nothing else useful. Status 6 means "wrong password",
// which on an ESP8266 also covers a four-way handshake that timed out, an AP that requires PMF,
// and an association the AP refused. The reason code separates them; a scan reports the AP's
// encryption mode, channel and signal. Both live here so the answer comes from the radio rather
// than from a guess.
//
// Build and flash:  pio run -d sdk/esp8266 -e scan -t upload

#include <Arduino.h>
#include <ESP8266WiFi.h>
#include <user_interface.h>   // wifi_set_macaddr: this core predates WiFi.setMacAddress

#ifndef HAVEN_WIFI_SSID
#define HAVEN_WIFI_SSID "YOUR_SSID"
#endif
#ifndef HAVEN_WIFI_PASS
#define HAVEN_WIFI_PASS "YOUR_PASS"
#endif

struct Ap {
    String ssid;
    uint8_t bssid[6];
    int channel;
    int rssi;
    uint8_t enc;
};

static Ap aps[40];
static int ap_count = 0;

static const char *auth_name(uint8_t mode) {
    switch (mode) {
        // The core's own mappings, not guesses: AUTH_WPA2_PSK -> CCMP, AUTH_WPA_WPA2_PSK -> AUTO,
        // and anything the SDK cannot name — a WPA3/SAE AP — falls through to -1 (0xFF here).
        case 2: return "WPA (TKIP)";
        case 4: return "WPA2 (CCMP)";
        case 5: return "WEP";
        case 7: return "OPEN";
        case 8: return "WPA/WPA2 mixed";
        case 255: return "unknown (WPA3/SAE?)";
        default: return "unrecognised";
    }
}

static const char *reason_name(uint8_t r) {
    switch (r) {
        case 2: return "AUTH_EXPIRE";
        case 6: return "NOT_AUTHED";
        case 7: return "NOT_ASSOCED";
        case 8: return "ASSOC_LEAVE (deauthenticated by the AP)";
        case 15: return "4WAY_HANDSHAKE_TIMEOUT (key or PMF mismatch)";
        case 200: return "BEACON_TIMEOUT";
        case 201: return "NO_AP_FOUND";
        case 202: return "AUTH_FAIL";
        case 203: return "ASSOC_FAIL";
        case 204: return "HANDSHAKE_TIMEOUT";
        default: return "other";
    }
}

static void on_disconnected(const WiFiEventStationModeDisconnected &e) {
    Serial.printf("  EVENT disconnected reason=%u (%s)\n", e.reason, reason_name(e.reason));
}
static void on_got_ip(const WiFiEventStationModeGotIP &e) {
    Serial.printf("  EVENT got ip %s\n", e.ip.toString().c_str());
}

void setup() {
    Serial.begin(115200);
    delay(300);
    Serial.println();
    Serial.println("=== haven wifi probe ===");
    Serial.printf("reset reason: %s\n", ESP.getResetReason().c_str());
    Serial.printf("heap %u, chip %08x, core %s\n", ESP.getFreeHeap(), ESP.getChipId(),
                  ESP.getCoreVersion().c_str());
    // Lengths, not values: a macro that lost a character is the difference between "the AP
    // refused" and "we asked to join a network that does not exist", and a log is public.
    Serial.printf("ssid \"%s\" (%u chars), password length %u\n", HAVEN_WIFI_SSID,
                  (unsigned)strlen(HAVEN_WIFI_SSID), (unsigned)strlen(HAVEN_WIFI_PASS));

    // `disconnect(true)` powers the radio down, and a station that is off never attempts — it sits
    // at status 7 forever with no events, which looks exactly like an AP refusing the key. Plain
    // `disconnect()` clears the association and leaves the station up.
    WiFi.persistent(false);
    WiFi.mode(WIFI_STA);
    WiFi.disconnect();
    delay(300);

    Serial.println("scanning...");
    int found = WiFi.scanNetworks(false, true);
    Serial.printf("networks: %d\n", found);
    for (int i = 0; i < found && ap_count < 40; i++) {
        Ap &ap = aps[ap_count++];
        ap.ssid = WiFi.SSID(i);
        memcpy(ap.bssid, WiFi.BSSID(i), 6);
        ap.channel = WiFi.channel(i);
        ap.rssi = WiFi.RSSI(i);
        ap.enc = WiFi.encryptionType(i);
        if (ap.ssid == HAVEN_WIFI_SSID) {
            Serial.printf("  MATCH ch%-2d %4d dBm %-20s %s\n", ap.channel, ap.rssi,
                          auth_name(ap.enc), WiFi.BSSIDstr(i).c_str());
        }
    }
    int matches = 0;
    for (int i = 0; i < ap_count; i++) {
        if (aps[i].ssid == HAVEN_WIFI_SSID) matches++;
    }
    // A station will not start while the scan results are still held: `begin()` returns, the status
    // stays disconnected, and no event ever fires. The results are copied into `aps` above, so the
    // SDK can drop them.
    WiFi.scanDelete();
    delay(200);

    Serial.printf("target visible: %s (%d bssid)\n", matches ? "yes" : "NO", matches);
    Serial.printf("mac: %s\n", WiFi.macAddress().c_str());

    WiFi.onStationModeDisconnected(on_disconnected);
    WiFi.onStationModeGotIP(on_got_ip);
}

void loop() {
    static bool done = false;
    if (done) {
        delay(1000);
        return;
    }
    done = true;

    // Strongest first, each BSSID pinned and tried alone. One SSID on two access points is a mesh,
    // and a station bounced between the nodes reports "disconnected" without naming a reason.
    int order[8], n = 0;
    for (int i = 0; i < ap_count; i++) {
        if (aps[i].ssid != HAVEN_WIFI_SSID) continue;
        int j = n++;
        while (j > 0 && aps[order[j - 1]].rssi < aps[i].rssi) {
            order[j] = order[j - 1];
            j--;
        }
        order[j] = i;
    }

    // Exactly what the firmware does: no channel, no BSSID, let the SDK choose. Every status
    // change and every SDK event is printed, because the reason code is the answer.
    Serial.println("plain begin(), as the firmware does it");
    WiFi.mode(WIFI_STA);
    WiFi.begin(HAVEN_WIFI_SSID, HAVEN_WIFI_PASS);
    int last = -1;
    for (int k = 0; k < 120; k++) {
        delay(500);
        int st = (int)WiFi.status();
        if (st != last) {
            Serial.printf("  t=%4.1fs status=%d\n", k * 0.5, st);
            last = st;
        }
        if (st == WL_CONNECTED) break;
    }
    if (WiFi.status() == WL_CONNECTED) {
        Serial.printf("  CONNECTED ip=%s rssi=%d (plain begin worked)\n",
                      WiFi.localIP().toString().c_str(), WiFi.RSSI());
        Serial.println("=== association test done ===");
        return;
    }
    WiFi.disconnect();
    delay(1000);

    for (int t = 0; t < n; t++) {
        const Ap &ap = aps[order[t]];
        Serial.printf("trying bssid %02x:%02x:%02x:%02x:%02x:%02x ch%d %ddBm\n", ap.bssid[0],
                      ap.bssid[1], ap.bssid[2], ap.bssid[3], ap.bssid[4], ap.bssid[5], ap.channel,
                      ap.rssi);
        WiFi.mode(WIFI_STA);   // a scan can leave the opmode as NULL; a station that is off
                               // accepts `begin()` and does nothing with it
        WiFi.begin(HAVEN_WIFI_SSID, HAVEN_WIFI_PASS, ap.channel, ap.bssid);
        delay(200);
        Serial.println("  sdk station config as it understands it:");
        WiFi.printDiag(Serial);
        int last = -1;
        for (int k = 0; k < 30; k++) {
            delay(500);
            int st = (int)WiFi.status();
            if (st != last) {
                Serial.printf("  status=%d\n", st);
                last = st;
            }
            if (st == WL_CONNECTED) break;
        }
        if (WiFi.status() == WL_CONNECTED) {
            Serial.printf("  CONNECTED ip=%s rssi=%d\n", WiFi.localIP().toString().c_str(),
                          WiFi.RSSI());
            break;
        }
        WiFi.disconnect();
        delay(1500);
    }
    Serial.printf("final status=%d (3 = connected)\n", (int)WiFi.status());
    Serial.println("=== association test done ===");
}
