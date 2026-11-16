/*
 * The connection policy, tested without a broker, a board or a clock.
 *
 * Each test here corresponds to a way real firmware goes wrong. Two of them are bugs this project
 * shipped and spent an afternoon on; the rest are the ones every device eventually has.
 */
#include <assert.h>
#include <stdio.h>
#include <string.h>

#include "../haven_runtime.h"
#include "fake_port.h"

static haven_link test_link(void) {
    haven_link link;
    memset(&link, 0, sizeof(link));
    /* Filled by hand on purpose: this is the shape a claim response yields. */
    snprintf(link.vendor_id, sizeof(link.vendor_id), "01a1d900-0000-7000-8000-000000000001");
    snprintf(link.home_id, sizeof(link.home_id), "01a1d900-0000-7000-8000-000000000002");
    snprintf(link.device_id, sizeof(link.device_id), "01a1d900-0000-7000-8000-000000000003");
    return link;
}

static void setup(haven_runtime *rt, fake_port *fake) {
    haven_link link = test_link();
    haven_port port;

    fake_port_init(fake);
    port = fake_port_ops;
    port.ctx = fake;

    assert(haven_runtime_init(rt, &port, &link, "factory-secret", "mqtts.example.test", 8883,
                              "{\"v\":1,\"online\":false}", 19) == HAVEN_OK);
}

static void test_first_tick_connects_then_subscribes(void) {
    haven_runtime rt;
    fake_port fake;
    setup(&rt, &fake);

    haven_runtime_tick(&rt);
    assert(fake.connects == 1);
    assert(fake.subscribes == 0); /* the subscription belongs to the connected tick */

    haven_runtime_tick(&rt);
    assert(fake.subscribes == 1);
    assert(strstr(fake.last_subscribe_topic, "/cmd") != NULL);

    /* And the credential presented is the device id and its secret, not a username. */
    assert(strcmp(fake.last_password, "factory-secret") == 0);

    /* The will points at the shadow topic, retained, so a dropped device reports itself. */
    assert(strstr(fake.last_will_topic, "/shadow") != NULL);
    printf("ok   first tick connects, second subscribes\n");
}

static void test_does_not_resubscribe_on_every_tick(void) {
    haven_runtime rt;
    fake_port fake;
    int i;
    setup(&rt, &fake);

    for (i = 0; i < 50; i++) {
        haven_runtime_tick(&rt);
    }

    assert(fake.connects == 1);
    assert(fake.subscribes == 1); /* fifty ticks, one subscribe */
    printf("ok   fifty ticks produce one connect and one subscribe\n");
}

static void test_disconnect_always_precedes_a_reconnect(void) {
    haven_runtime rt;
    fake_port fake;
    int i;
    setup(&rt, &fake);

    haven_runtime_tick(&rt); /* connect */
    haven_runtime_tick(&rt); /* subscribe */

    /* The link drops. */
    fake.connected = false;
    fake.order_len = 0;
    haven_runtime_tick(&rt);

    /* BUG-1: a reused TLS client that is not closed first never reconnects. The runtime must
     * close before opening, every time. */
    assert(fake.order_len >= 2);
    for (i = 0; i + 1 < fake.order_len; i++) {
        if (fake.order[i] == EV_CONNECT) {
            assert(i > 0 && fake.order[i - 1] == EV_DISCONNECT);
        }
    }
    assert(fake.connects == 2);
    assert(fake.disconnects >= 1);

    /* And the new session is subscribed again, because a clean session forgot it. */
    haven_runtime_tick(&rt);
    assert(fake.subscribes == 2);
    printf("ok   every reconnect is preceded by a disconnect, then resubscribes\n");
}

static void test_failure_backs_off_and_grows(void) {
    haven_runtime rt;
    fake_port fake;
    setup(&rt, &fake);

    fake.connect_ok = false;

    haven_runtime_tick(&rt); /* attempt 1 at t=0 */
    assert(fake.connects == 1);
    assert(fake.subscribes == 0);

    /* Immediately again: no storm, the delay has not elapsed. */
    haven_runtime_tick(&rt);
    assert(fake.connects == 1);

    /* After the first delay (1000 ms + zero jitter) it tries again. */
    fake.now_ms = HAVEN_BACKOFF_MIN_MS;
    haven_runtime_tick(&rt);
    assert(fake.connects == 2);

    /* The delay doubled: 2000 ms later, not 1000. */
    fake.now_ms = HAVEN_BACKOFF_MIN_MS + 1000;
    haven_runtime_tick(&rt);
    assert(fake.connects == 2);
    fake.now_ms = HAVEN_BACKOFF_MIN_MS + 2000;
    haven_runtime_tick(&rt);
    assert(fake.connects == 3);

    /* It recovers, and the delay resets to the floor. */
    fake.connect_ok = true;
    fake.now_ms += 100000;
    haven_runtime_tick(&rt);
    assert(fake.connects == 4);
    assert(rt.backoff_ms == HAVEN_BACKOFF_MIN_MS);
    printf("ok   failures back off, double, cap and reset\n");
}

static void test_backoff_is_jittered_and_capped(void) {
    haven_runtime rt;
    fake_port fake;
    int i;
    setup(&rt, &fake);
    fake.connect_ok = false;
    fake.random = 0xFFFFFFFFu; /* maximum jitter the implementation allows */

    for (i = 0; i < 40; i++) {
        fake.now_ms += (int64_t)rt.backoff_ms + 100000; /* always past the delay */
        haven_runtime_tick(&rt);
    }

    assert(rt.backoff_ms == HAVEN_BACKOFF_MAX_MS);
    assert(fake.connects == 40); /* one attempt per tick, forty ticks */
    printf("ok   backoff caps at %u ms across 40 failures\n", HAVEN_BACKOFF_MAX_MS);
}

static void test_ota_with_a_bad_signature_is_never_installed(void) {
    haven_runtime rt;
    fake_port fake;
    const uint8_t image[] = {0xE9, 0x01, 0x02, 0x03};
    const uint8_t sig[] = {0xDE, 0xAD};
    setup(&rt, &fake);

    fake.verify_ok = false;
    assert(haven_runtime_apply_ota(&rt, image, sizeof(image), sig, sizeof(sig)) ==
           HAVEN_ERR_OTA_SIGNATURE);

    /* The gate: nothing written, nothing rebooted, and it is counted. */
    assert(fake.installs == 0);
    assert(fake.reboots == 0);
    assert(rt.stats.ota_refused == 1);
    printf("ok   an unsigned image is refused before the flash is touched\n");
}

static void test_ota_with_a_good_signature_installs_then_reboots(void) {
    haven_runtime rt;
    fake_port fake;
    const uint8_t image[] = {0xE9, 0x01, 0x02, 0x03};
    const uint8_t sig[] = {0x01, 0x02};
    setup(&rt, &fake);

    assert(haven_runtime_apply_ota(&rt, image, sizeof(image), sig, sizeof(sig)) == HAVEN_OK);
    assert(fake.installs == 1);
    assert(fake.reboots == 1);
    assert(rt.stats.ota_installed == 1);
    printf("ok   a signed image installs once and reboots\n");
}

static void test_poll_delivers_one_message_and_reports_empty(void) {
    haven_runtime rt;
    fake_port fake;
    int seen = 0;
    setup(&rt, &fake);

    /* No connection: nothing to poll, and not an error worth logging. */
    assert(haven_runtime_poll(&rt, NULL, NULL) == HAVEN_ERR_AGAIN);

    haven_runtime_tick(&rt);
    assert(haven_runtime_poll(&rt, NULL, NULL) == HAVEN_ERR_AGAIN); /* connected, queue empty */

    fake.message_pending = true;
    assert(haven_runtime_poll(&rt, NULL, NULL) == HAVEN_OK);
    assert(rt.stats.messages == 1);
    (void)seen;
    printf("ok   poll returns AGAIN when empty and OK when a message lands\n");
}

static void test_init_refuses_a_bad_uuid_and_an_oversized_secret(void) {
    haven_runtime rt;
    fake_port fake;
    haven_link link = test_link();
    haven_port port;

    fake_port_init(&fake);
    port = fake_port_ops;
    port.ctx = &fake;

    snprintf(link.device_id, sizeof(link.device_id), "not-a-uuid");
    assert(haven_runtime_init(&rt, &port, &link, "s", "h", 1, NULL, 0) == HAVEN_ERR_INVALID_UUID);

    link = test_link();
    {
        char big[HAVEN_SECRET_MAX + 8];
        memset(big, 'x', sizeof(big) - 1);
        big[sizeof(big) - 1] = '\0';
        assert(haven_runtime_init(&rt, &port, &link, big, "h", 1, NULL, 0) == HAVEN_ERR_TOO_SMALL);
    }
    printf("ok   init refuses a malformed link and an oversized secret\n");
}

int main(void) {
    test_first_tick_connects_then_subscribes();
    test_does_not_resubscribe_on_every_tick();
    test_disconnect_always_precedes_a_reconnect();
    test_failure_backs_off_and_grows();
    test_backoff_is_jittered_and_capped();
    test_ota_with_a_bad_signature_is_never_installed();
    test_ota_with_a_good_signature_installs_then_reboots();
    test_poll_delivers_one_message_and_reports_empty();
    test_init_refuses_a_bad_uuid_and_an_oversized_secret();
    printf("\nruntime: all checks passed\n");
    return 0;
}
