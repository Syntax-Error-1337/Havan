/*
 * haven_runtime.h — the connection policy a vendor should not have to write.
 *
 * A Haven device does the same six things on every board: connect with its id and secret,
 * subscribe to its command topic, republish a shadow, keep the session alive, back off when the
 * network is gone, and apply an OTA only when the image is signed by its vendor. Every firmware
 * that writes that loop itself writes it slightly differently, and two of the differences cost
 * this project real time: a TLS client reused without closing never reconnected, and a device
 * whose "am I connected" test was wrong evicted its own live session every twelve seconds.
 *
 * So the loop lives here, driven by one `tick` and one port (`haven_port.h`). The vendor's main
 * loop becomes:
 *
 *     haven_runtime_tick(&rt);          // connect / subscribe / back off, as needed
 *     haven_runtime_poll(&rt);          // hand inbound commands to the application
 *
 * MIT-licensed like the rest of the SDK surface; see the repository LICENSE.
 */
#ifndef HAVEN_RUNTIME_H
#define HAVEN_RUNTIME_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "haven.h"
#include "haven_port.h"

#ifdef __cplusplus
extern "C" {
#endif

/* First retry delay, and the ceiling it doubles up to. Chosen so a device that loses Wi-Fi for a
 * minute reconnects promptly, and one that loses it for an hour does not hammer the broker. */
#define HAVEN_BACKOFF_MIN_MS 1000u
#define HAVEN_BACKOFF_MAX_MS 60000u

/* What the runtime did, so a vendor can log it and a test can assert it. */
typedef struct {
    uint32_t connects;        /* successful connections */
    uint32_t connect_failures;
    uint32_t subscribes;      /* command subscriptions made; should equal connects */
    uint32_t messages;        /* inbound messages handed to the application */
    uint32_t ota_refused;     /* images whose signature did not verify */
    uint32_t ota_installed;
} haven_runtime_stats;

typedef struct {
    const haven_port *port;

    haven_link link;
    char secret[HAVEN_SECRET_MAX];
    char host[64];
    uint16_t port_number;

    /* Built once in `init`, so no call site spells a topic. */
    char cmd_topic[HAVEN_TOPIC_MAX];
    char shadow_topic[HAVEN_TOPIC_MAX];
    char will_payload[HAVEN_ENVELOPE_MAX];
    size_t will_len;

    bool connected_once;
    bool subscribed;
    uint64_t next_attempt_ms;
    uint32_t backoff_ms;

    haven_runtime_stats stats;
} haven_runtime;

/* Prepare a runtime. `will_payload` is the retained shadow body the broker publishes if this
 * device disappears; pass NULL to run without a last will (a benchmark, not a product). */
haven_status haven_runtime_init(haven_runtime *rt, const haven_port *port, const haven_link *link,
                                const char *secret, const char *host, uint16_t port_number,
                                const char *will_payload, size_t will_len);

/* Drive the connection: connect when it is time, subscribe once per connection, back off on
 * failure. Safe to call as often as the main loop runs. */
void haven_runtime_tick(haven_runtime *rt);

/* Deliver at most one inbound message. Returns HAVEN_OK when `on_message` was called, and
 * HAVEN_ERR_AGAIN when there was nothing (or no connection). */
haven_status haven_runtime_poll(haven_runtime *rt, void (*on_message)(const haven_message *msg,
                                                                     void *user),
                                void *user);

/* Apply an OTA image only if the vendor's signature verifies. On success the device is
 * rebooted through the port and the call does not return; on failure nothing is installed and
 * the reason is counted. */
haven_status haven_runtime_apply_ota(haven_runtime *rt, const uint8_t *image, size_t len,
                                     const uint8_t *sig, size_t sig_len);

/* True while a session is up, according to the port. */
bool haven_runtime_is_connected(const haven_runtime *rt);

#ifdef __cplusplus
}
#endif

#endif /* HAVEN_RUNTIME_H */
