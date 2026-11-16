/*
 * A port that records what the runtime asked it to do. Test-only.
 *
 * This is the whole argument for the port existing: the connection policy can be tested without
 * a broker, a chip, or a clock, by scripting the port's answers.
 */
#ifndef FAKE_PORT_H
#define FAKE_PORT_H

#include "../haven_port.h"

typedef enum {
    EV_CONNECT = 1,
    EV_DISCONNECT,
    EV_SUBSCRIBE,
    EV_PUBLISH
} fake_event;

typedef struct {
    /* Script: what the next connect() answers, and whether a session is up. */
    bool connect_ok;
    bool connected;

    /* Counters the tests assert on. */
    int connects;
    int disconnects;
    int subscribes;
    int publishes;
    int installs;
    int reboots;
    int messages;

    /* The order operations happened in, to catch "connected without closing first". */
    fake_event order[32];
    int order_len;

    /* The fake clock and a fixed source of randomness, so backoff is deterministic. */
    int64_t now_ms;
    uint32_t random;

    /* OTA script. */
    bool verify_ok;
    bool install_ok;

    /* One queued inbound message, delivered by the next next_message() call. */
    bool message_pending;

    char last_subscribe_topic[HAVEN_TOPIC_MAX];
    char last_publish_topic[HAVEN_TOPIC_MAX];
    char last_password[HAVEN_SECRET_MAX];
    char last_will_topic[HAVEN_TOPIC_MAX];
} fake_port;

extern const haven_port fake_port_ops;

void fake_port_init(fake_port *p);

#endif /* FAKE_PORT_H */
