/*
 * haven_runtime.c — the connection loop, once, for every board.
 *
 * See haven_runtime.h for why this exists. The two rules that encode the bugs this project
 * actually hit are marked BUG-1 and BUG-2 below; both have a test in tests/test_runtime.c.
 *
 * MIT-licensed like the rest of the SDK surface; see the repository LICENSE.
 */
#include "haven_runtime.h"

#include <string.h>

static void copy_str(char *dst, size_t cap, const char *src) {
    size_t n;
    if (cap == 0) {
        return;
    }
    n = strlen(src);
    if (n >= cap) {
        n = cap - 1;
    }
    memcpy(dst, src, n);
    dst[n] = '\0';
}

haven_status haven_runtime_init(haven_runtime *rt, const haven_port *port, const haven_link *link,
                                const char *secret, const char *host, uint16_t port_number,
                                const char *will_payload, size_t will_len) {
    if (rt == NULL || port == NULL || link == NULL || secret == NULL) {
        return HAVEN_ERR_INVALID_ARG;
    }
    if (!haven_is_uuid(link->vendor_id) || !haven_is_uuid(link->home_id) ||
        !haven_is_uuid(link->device_id)) {
        return HAVEN_ERR_INVALID_UUID;
    }
    if (strlen(secret) >= sizeof(rt->secret)) {
        return HAVEN_ERR_TOO_SMALL;
    }

    memset(rt, 0, sizeof(*rt));
    rt->port = port;
    rt->link = *link;
    copy_str(rt->secret, sizeof(rt->secret), secret);
    copy_str(rt->host, sizeof(rt->host), host != NULL ? host : "");
    rt->port_number = port_number;

    if (haven_topic_cmd(&rt->link, rt->cmd_topic, sizeof(rt->cmd_topic)) != HAVEN_OK ||
        haven_topic_shadow(&rt->link, rt->shadow_topic, sizeof(rt->shadow_topic)) != HAVEN_OK) {
        return HAVEN_ERR_TOO_SMALL;
    }

    if (will_payload != NULL && will_len > 0) {
        if (will_len >= sizeof(rt->will_payload)) {
            return HAVEN_ERR_TOO_SMALL;
        }
        memcpy(rt->will_payload, will_payload, will_len);
        rt->will_len = will_len;
    }

    rt->backoff_ms = HAVEN_BACKOFF_MIN_MS;
    rt->next_attempt_ms = 0;
    return HAVEN_OK;
}

/* Exponential backoff with jitter, capped.
 *
 * The jitter matters more than the cap: a hundred devices that lost the same router reconnect on
 * the same second, and the broker sees a thundering herd exactly when it is least able to serve
 * one. Quarter-width jitter is enough to spread them without making the delay unpredictable. */
static void schedule_retry(haven_runtime *rt, int64_t now) {
    uint32_t base = rt->backoff_ms;
    uint32_t jitter = 0;

    if (rt->port->random_u32 != NULL) {
        jitter = rt->port->random_u32(rt->port->ctx) % (base / 4 + 1);
    }
    rt->next_attempt_ms = (uint64_t)now + base + jitter;

    base = base * 2;
    rt->backoff_ms = base > HAVEN_BACKOFF_MAX_MS ? HAVEN_BACKOFF_MAX_MS : base;
}

void haven_runtime_tick(haven_runtime *rt) {
    const haven_port *port;
    haven_connection conn;
    int64_t now;

    if (rt == NULL || rt->port == NULL) {
        return;
    }
    port = rt->port;
    now = port->now_ms(port->ctx);

    if (port->is_connected(port->ctx)) {
        /* Subscribe once per connection, not once per loop. Resubscribing on every tick is the
         * shape of bug BUG-2's cousin: it looks harmless and it floods the broker. */
        if (!rt->subscribed) {
            if (port->subscribe(port->ctx, rt->cmd_topic, 1) == HAVEN_OK) {
                rt->subscribed = true;
                rt->stats.subscribes++;
            }
        }
        return;
    }

    /* Not connected: whatever we believed about the subscription is now stale. */
    rt->subscribed = false;

    if (now < (int64_t)rt->next_attempt_ms) {
        return;
    }

    /* BUG-1: close before opening, always.
     *
     * A TLS client left mid-handshake keeps its state, and the next `connect` returns without
     * doing anything — so the device "retries" forever and never reconnects. On this project
     * that presented as a board that reconnected only when its serial port was opened. */
    port->disconnect(port->ctx);

    memset(&conn, 0, sizeof(conn));
    conn.host = rt->host;
    conn.port = rt->port_number;
    conn.client_id = rt->link.device_id;
    conn.username = rt->link.device_id;
    conn.password = rt->secret;
    conn.will_topic = rt->will_len > 0 ? rt->shadow_topic : NULL;
    conn.will_payload = rt->will_len > 0 ? rt->will_payload : NULL;
    conn.will_len = rt->will_len;
    conn.will_retain = true;
    /* A clean session with an explicit resubscribe on connect is the contract's rule: it makes
     * the device's state after a reboot identical to its state after a reconnect, which is the
     * only version of this a firmware author can reason about. */
    conn.clean_session = true;
    conn.keepalive_s = 90;

    if (port->connect(port->ctx, &conn) == HAVEN_OK) {
        rt->stats.connects++;
        rt->connected_once = true;
        rt->backoff_ms = HAVEN_BACKOFF_MIN_MS;
        rt->next_attempt_ms = 0;
        return;
    }

    rt->stats.connect_failures++;
    schedule_retry(rt, now);
}

haven_status haven_runtime_poll(haven_runtime *rt, void (*on_message)(const haven_message *msg,
                                                                     void *user),
                                void *user) {
    haven_message msg;
    haven_status status;

    if (rt == NULL || rt->port == NULL) {
        return HAVEN_ERR_INVALID_ARG;
    }
    if (!rt->port->is_connected(rt->port->ctx)) {
        return HAVEN_ERR_AGAIN;
    }

    memset(&msg, 0, sizeof(msg));
    status = rt->port->next_message(rt->port->ctx, &msg);
    if (status != HAVEN_OK) {
        return status;
    }

    rt->stats.messages++;
    if (on_message != NULL) {
        on_message(&msg, user);
    }
    return HAVEN_OK;
}

haven_status haven_runtime_apply_ota(haven_runtime *rt, const uint8_t *image, size_t len,
                                     const uint8_t *sig, size_t sig_len) {
    const haven_port *port;

    if (rt == NULL || rt->port == NULL || image == NULL || len == 0) {
        return HAVEN_ERR_INVALID_ARG;
    }
    port = rt->port;

    /* Verify before install, and never the other way round. An image that is written to flash
     * before its signature is checked has already replaced the firmware — the check would then
     * be a report, not a gate. A device with no verifier refuses every image rather than
     * trusting one; that is the safe direction for a missing capability. */
    if (port->ota_verify == NULL ||
        port->ota_verify(port->ctx, image, len, sig, sig_len) != HAVEN_OK) {
        rt->stats.ota_refused++;
        return HAVEN_ERR_OTA_SIGNATURE;
    }
    if (port->ota_install == NULL || port->ota_install(port->ctx, image, len) != HAVEN_OK) {
        return HAVEN_ERR_INVALID_ARG;
    }

    rt->stats.ota_installed++;
    if (port->reboot != NULL) {
        port->reboot(port->ctx);
    }
    return HAVEN_OK;
}

bool haven_runtime_is_connected(const haven_runtime *rt) {
    if (rt == NULL || rt->port == NULL) {
        return false;
    }
    return rt->port->is_connected(rt->port->ctx);
}
