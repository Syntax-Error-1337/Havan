#include "fake_port.h"

#include <string.h>

static void record(fake_port *p, fake_event e) {
    if (p->order_len < (int)(sizeof(p->order) / sizeof(p->order[0]))) {
        p->order[p->order_len++] = e;
    }
}

static void copy(char *dst, size_t cap, const char *src) {
    size_t n = src != NULL ? strlen(src) : 0;
    if (n >= cap) {
        n = cap - 1;
    }
    if (n > 0 && src != NULL) {
        memcpy(dst, src, n);
    }
    dst[n] = '\0';
}

static haven_status f_connect(void *ctx, const haven_connection *c) {
    fake_port *p = (fake_port *)ctx;
    record(p, EV_CONNECT);
    p->connects++;
    if (!p->connect_ok) {
        p->connected = false;
        return HAVEN_ERR_INVALID_ARG;
    }
    p->connected = true;
    copy(p->last_password, sizeof(p->last_password), c->password);
    copy(p->last_will_topic, sizeof(p->last_will_topic), c->will_topic);
    return HAVEN_OK;
}

static void f_disconnect(void *ctx) {
    fake_port *p = (fake_port *)ctx;
    record(p, EV_DISCONNECT);
    p->disconnects++;
    p->connected = false;
}

static bool f_is_connected(void *ctx) {
    return ((fake_port *)ctx)->connected;
}

static haven_status f_subscribe(void *ctx, const char *topic, int qos) {
    fake_port *p = (fake_port *)ctx;
    (void)qos;
    record(p, EV_SUBSCRIBE);
    p->subscribes++;
    copy(p->last_subscribe_topic, sizeof(p->last_subscribe_topic), topic);
    return HAVEN_OK;
}

static haven_status f_publish(void *ctx, const char *topic, const void *payload, size_t len,
                              int qos, bool retain) {
    fake_port *p = (fake_port *)ctx;
    (void)payload;
    (void)len;
    (void)qos;
    (void)retain;
    record(p, EV_PUBLISH);
    p->publishes++;
    copy(p->last_publish_topic, sizeof(p->last_publish_topic), topic);
    return HAVEN_OK;
}

static haven_status f_next_message(void *ctx, haven_message *out) {
    fake_port *p = (fake_port *)ctx;
    if (!p->message_pending) {
        return HAVEN_ERR_AGAIN;
    }
    p->message_pending = false;
    p->messages++;
    copy(out->topic, sizeof(out->topic), "haven/v1/a/b/c/cmd");
    out->payload = "{\"v\":1,\"id\":\"cmd-1\",\"op\":\"set\",\"dp\":\"1\",\"value\":true}";
    out->len = strlen(out->payload);
    out->qos = 1;
    out->retain = false;
    return HAVEN_OK;
}

static int64_t f_now_ms(void *ctx) {
    return ((fake_port *)ctx)->now_ms;
}

static uint32_t f_random_u32(void *ctx) {
    return ((fake_port *)ctx)->random;
}

static haven_status f_ota_verify(void *ctx, const uint8_t *image, size_t len, const uint8_t *sig,
                                 size_t sig_len) {
    fake_port *p = (fake_port *)ctx;
    (void)image;
    (void)len;
    (void)sig;
    (void)sig_len;
    return p->verify_ok ? HAVEN_OK : HAVEN_ERR_OTA_SIGNATURE;
}

static haven_status f_ota_install(void *ctx, const uint8_t *image, size_t len) {
    fake_port *p = (fake_port *)ctx;
    (void)image;
    (void)len;
    p->installs++;
    return p->install_ok ? HAVEN_OK : HAVEN_ERR_INVALID_ARG;
}

static void f_reboot(void *ctx) {
    ((fake_port *)ctx)->reboots++;
}

const haven_port fake_port_ops = {
    NULL, /* ctx is set by fake_port_init */
    f_connect,
    f_disconnect,
    f_is_connected,
    f_subscribe,
    f_publish,
    f_next_message,
    f_now_ms,
    f_random_u32,
    f_ota_verify,
    f_ota_install,
    f_reboot,
};

void fake_port_init(fake_port *p) {
    memset(p, 0, sizeof(*p));
    p->connect_ok = true;
    p->verify_ok = true;
    p->install_ok = true;
    p->random = 0; /* deterministic backoff unless a test says otherwise */
}
