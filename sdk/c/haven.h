/*
 * haven.h — the device side of the Haven protocol, in C.
 *
 * A vendor's firmware needs four things from this platform: the topics to publish on, the
 * envelopes to publish, the refusal shape when a command makes no sense, and the claim body
 * that gets it on the network. This file provides exactly those and nothing else.
 *
 * Deliberately no transport. An ESP32 uses esp_mqtt_client and esp_http_client; a Linux
 * gateway uses libmosquitto and curl; a test uses neither. Baking one in would make this a
 * framework instead of a reference, and every vendor would fork it.
 *
 * Rules the implementation keeps, because they are the ones firmware gets wrong:
 *   - topics are built, never formatted by hand at the call site (a typo in a topic is a
 *     message nobody receives, with no error anywhere);
 *   - every buffer is caller-supplied or caller-freed, and every function reports truncation
 *     instead of silently writing a partial envelope;
 *   - a datapoint value is typed, not a string, so `"true"` and `true` cannot both reach the
 *     wire.
 *
 * MIT-licensed like the rest of the SDK surface; see the repository LICENSE.
 */
#ifndef HAVEN_H
#define HAVEN_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define HAVEN_WIRE_VERSION 1

/* Longest id a topic is built from: 36-character uuid plus a NUL. */
#define HAVEN_UUID_LEN 37

/* Enough for a shadow with four datapoints and a firmware string. Envelopes larger than this
 * are a device reporting something it should not, not a buffer to grow. */
#define HAVEN_ENVELOPE_MAX 1024

/* A datapoint value. The kind decides the field written; the union means `true` and 1 cannot
 * be confused for each other the way they can with a bare string. */
typedef enum {
    HAVEN_DP_BOOL = 0,
    HAVEN_DP_INT,
    HAVEN_DP_FLOAT,
    HAVEN_DP_TEXT
} haven_dp_kind;

typedef struct {
    const char *dp;        /* "1", "2" — the datapoint id from the product's thing model */
    haven_dp_kind kind;
    union {
        bool b;
        int64_t i;
        double f;
        const char *text;
    } value;
} haven_dp;

/* Where this device lives. Filled in from the claim response; never hand-assembled. */
typedef struct {
    char vendor_id[HAVEN_UUID_LEN];
    char home_id[HAVEN_UUID_LEN];
    char device_id[HAVEN_UUID_LEN];
} haven_link;

/* What a claim needs. `serial` and `secret` come from the factory; `ticket` from the app. */
typedef struct {
    const char *product_key;
    const char *serial;
    const char *secret;
    const char *ticket;
    const char *firmware;
} haven_claim_request;

typedef enum {
    HAVEN_OK = 0,
    HAVEN_ERR_INVALID_ARG,
    HAVEN_ERR_TOO_SMALL,   /* the caller's buffer cannot hold the result */
    HAVEN_ERR_INVALID_UUID
} haven_status;

/* ---- topics ---------------------------------------------------------------- */

haven_status haven_topic_event(const haven_link *link, char *out, size_t out_len);
haven_status haven_topic_shadow(const haven_link *link, char *out, size_t out_len);
haven_status haven_topic_ack(const haven_link *link, char *out, size_t out_len);
haven_status haven_topic_ota(const haven_link *link, char *out, size_t out_len);
haven_status haven_topic_cmd(const haven_link *link, char *out, size_t out_len);

/* ---- envelopes ------------------------------------------------------------- */

/* A state report on the event lane: `{"v":1,"ts":…,"pts":{"1":true}}`. */
haven_status haven_state_event(const haven_dp *pts, size_t count, int64_t ts, char *out,
                               size_t out_len);

/* A retained shadow: the same points plus `online` and the firmware string, so a device that
 * reboots is current immediately instead of waiting for its next report. */
haven_status haven_shadow(const haven_dp *pts, size_t count, int64_t ts, bool online,
                          const char *firmware, char *out, size_t out_len);

/* The ack for a command that was applied. */
haven_status haven_ack(const char *command_id, const char *dp, const haven_dp *value,
                       int64_t ts, char *out, size_t out_len);

/* The refusal for a command that was not: `unknown_dp`, `bad_type`, `out_of_range`,
 * `not_writable`, `unsupported`. A typed reason is what lets the app say something true. */
haven_status haven_refusal(const char *command_id, int64_t ts, const char *code,
                           const char *message, char *out, size_t out_len);

/* OTA progress, so a campaign can see a device mid-flight rather than only at the end. */
haven_status haven_ota_report(const char *campaign_id, const char *state, int pct, int64_t ts,
                              char *out, size_t out_len);

/* The JSON body for `POST /provision/claim`. */
haven_status haven_claim_body(const haven_claim_request *request, char *out, size_t out_len);

/* ---- helpers the call site needs ------------------------------------------- */

/* Refusal codes as strings, so firmware does not spell them inline. */
const char *haven_refusal_unknown_dp(void);
const char *haven_refusal_bad_type(void);
const char *haven_refusal_out_of_range(void);
const char *haven_refusal_not_writable(void);

/* True when `id` is a 36-character uuid in 8-4-4-4-12 form. Cheap, and it stops a device with
 * a truncated id from publishing to a topic nobody is subscribed to. */
bool haven_is_uuid(const char *id);

#ifdef __cplusplus
}
#endif

#endif /* HAVEN_H */
