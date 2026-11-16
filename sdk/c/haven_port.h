/*
 * haven_port.h — everything the SDK cannot know: the wire and the chip.
 *
 * `haven.h` refuses to bake in a transport, and that rule stands: this file does not implement
 * MQTT or TLS either. It declares the *handful of things* a runtime needs from whatever does,
 * as function pointers the vendor fills in. An ESP32 points them at esp_mqtt_client; a Linux
 * gateway at libmosquitto; a test at `haven_port_fake`.
 *
 * Why a port at all, when the alternative is "let each firmware write its connection loop":
 * because the loop is where the bugs are. The two that cost the most on this project — a reused
 * TLS client that never reconnected, and a device that evicted its own live session — were both
 * in hand-written connect code in a sketch. Policy belongs in `haven_runtime.c`, once; the
 * vendor supplies only what is genuinely chip-specific.
 *
 * Contract for implementors:
 *   - `connect` returns HAVEN_OK only after CONNACK, and must be preceded by `disconnect`
 *     (the runtime enforces the order, because skipping it is the bug above);
 *   - `next_message` returns HAVEN_ERR_AGAIN when there is nothing to deliver, and the payload
 *     it hands back stays valid until the next call;
 *   - `ota_verify` must check the vendor's signature over the image and return HAVEN_OK only
 *     when it is valid — the runtime installs whatever this approves;
 *   - no function may block for longer than its stated timeout.
 *
 * MIT-licensed like the rest of the SDK surface; see the repository LICENSE.
 */
#ifndef HAVEN_PORT_H
#define HAVEN_PORT_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "haven.h"

#ifdef __cplusplus
extern "C" {
#endif

/* A topic with every id filled in: 5 segments of at most a uuid plus separators. */
#define HAVEN_TOPIC_MAX 160
/* The factory secret: enough for a 64-character hex string and a NUL. */
#define HAVEN_SECRET_MAX 72
/* The longest inbound command payload the runtime will hand to the application. */
#define HAVEN_MESSAGE_MAX 1024

/* What `connect` needs to open a session. */
typedef struct {
    const char *host;
    uint16_t port;
    const char *client_id;
    /* The device id and secret: the MQTT credential, checked by the platform's auth hook. */
    const char *username;
    const char *password;
    /* Last will: the broker publishes this on the shadow topic when the device drops, which is
     * how the platform learns a device is offline without waiting for a keepalive to expire. */
    const char *will_topic;
    const char *will_payload;
    size_t will_len;
    bool will_retain;
    bool clean_session;
    uint16_t keepalive_s;
} haven_connection;

/* One inbound message. `payload` is borrowed: valid until the next `next_message` call. */
typedef struct {
    char topic[HAVEN_TOPIC_MAX];
    const char *payload;
    size_t len;
    int qos;
    bool retain;
} haven_message;

typedef struct haven_port {
    void *ctx;

    haven_status (*connect)(void *ctx, const haven_connection *c);
    void (*disconnect)(void *ctx);
    bool (*is_connected)(void *ctx);

    haven_status (*subscribe)(void *ctx, const char *topic, int qos);
    haven_status (*publish)(void *ctx, const char *topic, const void *payload, size_t len,
                            int qos, bool retain);

    /* Deliver one inbound message, or HAVEN_ERR_AGAIN when there is none. */
    haven_status (*next_message)(void *ctx, haven_message *out);

    /* Platform services. */
    int64_t (*now_ms)(void *ctx);
    uint32_t (*random_u32)(void *ctx);

    /* True only when the vendor's signature over `image` verifies. */
    haven_status (*ota_verify)(void *ctx, const uint8_t *image, size_t len, const uint8_t *sig,
                               size_t sig_len);
    haven_status (*ota_install)(void *ctx, const uint8_t *image, size_t len);
    void (*reboot)(void *ctx);
} haven_port;

#ifdef __cplusplus
}
#endif

#endif /* HAVEN_PORT_H */
