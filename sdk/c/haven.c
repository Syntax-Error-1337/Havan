/*
 * haven.c — see haven.h. No allocation, no globals, no transport.
 *
 * Every function writes into a caller buffer and reports truncation. The alternative —
 * allocating and returning a pointer — is how firmware leaks a few hundred bytes per report
 * until it stops, and it is the first thing to go wrong on a device with 320 KB of RAM.
 */
#include "haven.h"

#include <stdio.h>
#include <string.h>

/* ---- small helpers --------------------------------------------------------- */

static bool valid(const void *p) { return p != NULL; }

bool haven_is_uuid(const char *id) {
    if (!valid(id)) return false;
    size_t len = strlen(id);
    if (len != 36) return false;

    for (size_t i = 0; i < len; i++) {
        char c = id[i];
        if (i == 8 || i == 13 || i == 18 || i == 23) {
            if (c != '-') return false;
        } else if (!((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F'))) {
            return false;
        }
    }
    return true;
}

/* snprintf returns what it *would* have written, which is how truncation is detected instead
 * of avoided. A partial JSON envelope is worse than no envelope: the broker accepts it and the
 * ingest lane logs a decode error nobody reads. */
static haven_status finish(int written, size_t out_len) {
    if (written < 0) return HAVEN_ERR_INVALID_ARG;
    if ((size_t)written >= out_len) return HAVEN_ERR_TOO_SMALL;
    return HAVEN_OK;
}

static haven_status build_topic(const haven_link *link, const char *lane, char *out,
                                size_t out_len) {
    if (!valid(link) || !valid(lane) || !valid(out)) return HAVEN_ERR_INVALID_ARG;
    if (!haven_is_uuid(link->vendor_id) || !haven_is_uuid(link->home_id) ||
        !haven_is_uuid(link->device_id)) {
        return HAVEN_ERR_INVALID_UUID;
    }

    /* haven/v1/{vendor}/{home}/{device}/{lane} — the same shape the server subscribes to, built
     * in one place so a firmware bug cannot invent a segment. The `v1` is not decoration: the
     * worker subscribes to `$share/ingest/haven/v1/+/+/+/+`, so a topic without it is published
     * to nobody and the device looks connected and offline at the same time. */
    int n = snprintf(out, out_len, "haven/v1/%s/%s/%s/%s", link->vendor_id, link->home_id,
                     link->device_id, lane);
    return finish(n, out_len);
}

/* One datapoint as `"<dp>":<value>`. Numbers are written with enough precision to survive a
 * round trip: a temperature at %.1f loses the second decimal every report. */
static haven_status write_dp(const haven_dp *p, char *out, size_t out_len, int *written) {
    if (!valid(p) || !valid(p->dp)) return HAVEN_ERR_INVALID_ARG;

    int n = 0;
    switch (p->kind) {
        case HAVEN_DP_BOOL:
            n = snprintf(out, out_len, "\"%s\":%s", p->dp, p->value.b ? "true" : "false");
            break;
        case HAVEN_DP_INT:
            n = snprintf(out, out_len, "\"%s\":%lld", p->dp, (long long)p->value.i);
            break;
        case HAVEN_DP_FLOAT:
            n = snprintf(out, out_len, "\"%s\":%.6g", p->dp, p->value.f);
            break;
        case HAVEN_DP_TEXT:
            if (!valid(p->value.text)) return HAVEN_ERR_INVALID_ARG;
            /* No escaping: a text value containing a quote is a mistake at the call site, and
             * silently mangling it into valid JSON would hide it. */
            if (strchr(p->value.text, '"') || strchr(p->value.text, '\\')) {
                return HAVEN_ERR_INVALID_ARG;
            }
            n = snprintf(out, out_len, "\"%s\":\"%s\"", p->dp, p->value.text);
            break;
        default:
            return HAVEN_ERR_INVALID_ARG;
    }

    haven_status st = finish(n, out_len);
    if (st == HAVEN_OK && valid(written)) *written = n;
    return st;
}

/* `{"1":true,"2":80}` into `out`, returning the bytes written in `*used`. */
static haven_status write_points(const haven_dp *pts, size_t count, char *out, size_t out_len,
                                 size_t *used) {
    if (count > 0 && !valid(pts)) return HAVEN_ERR_INVALID_ARG;

    size_t at = 0;
    if (out_len < 2) return HAVEN_ERR_TOO_SMALL;
    out[at++] = '{';

    for (size_t i = 0; i < count; i++) {
        if (i > 0) {
            if (at + 1 >= out_len) return HAVEN_ERR_TOO_SMALL;
            out[at++] = ',';
        }
        int written = 0;
        haven_status st = write_dp(&pts[i], out + at, out_len - at, &written);
        if (st != HAVEN_OK) return st;
        at += (size_t)written;
    }

    if (at + 2 > out_len) return HAVEN_ERR_TOO_SMALL;
    out[at++] = '}';
    out[at] = '\0';
    *used = at;
    return HAVEN_OK;
}

/* ---- topics ---------------------------------------------------------------- */

haven_status haven_topic_event(const haven_link *link, char *out, size_t out_len) {
    return build_topic(link, "evt", out, out_len);
}
haven_status haven_topic_shadow(const haven_link *link, char *out, size_t out_len) {
    return build_topic(link, "shadow", out, out_len);
}
haven_status haven_topic_ack(const haven_link *link, char *out, size_t out_len) {
    return build_topic(link, "ack", out, out_len);
}
haven_status haven_topic_ota(const haven_link *link, char *out, size_t out_len) {
    return build_topic(link, "ota", out, out_len);
}
haven_status haven_topic_cmd(const haven_link *link, char *out, size_t out_len) {
    return build_topic(link, "cmd", out, out_len);
}

/* ---- envelopes ------------------------------------------------------------- */

haven_status haven_state_event(const haven_dp *pts, size_t count, int64_t ts, char *out,
                               size_t out_len) {
    if (!valid(out)) return HAVEN_ERR_INVALID_ARG;

    char points[HAVEN_ENVELOPE_MAX / 2];
    size_t used = 0;
    haven_status st = write_points(pts, count, points, sizeof points, &used);
    if (st != HAVEN_OK) return st;
    (void)used;

    int n = snprintf(out, out_len, "{\"v\":%d,\"ts\":%lld,\"pts\":%s}", HAVEN_WIRE_VERSION,
                     (long long)ts, points);
    return finish(n, out_len);
}

haven_status haven_shadow(const haven_dp *pts, size_t count, int64_t ts, bool online,
                          const char *firmware, char *out, size_t out_len) {
    if (!valid(out) || !valid(firmware)) return HAVEN_ERR_INVALID_ARG;
    if (strchr(firmware, '"')) return HAVEN_ERR_INVALID_ARG;

    char points[HAVEN_ENVELOPE_MAX / 2];
    size_t used = 0;
    haven_status st = write_points(pts, count, points, sizeof points, &used);
    if (st != HAVEN_OK) return st;
    (void)used;

    int n = snprintf(out, out_len,
                     "{\"v\":%d,\"ts\":%lld,\"online\":%s,\"fw\":\"%s\",\"pts\":%s}",
                     HAVEN_WIRE_VERSION, (long long)ts, online ? "true" : "false", firmware,
                     points);
    return finish(n, out_len);
}

haven_status haven_ack(const char *command_id, const char *dp, const haven_dp *value, int64_t ts,
                       char *out, size_t out_len) {
    if (!valid(command_id) || !valid(dp) || !valid(value) || !valid(out)) {
        return HAVEN_ERR_INVALID_ARG;
    }
    if (!haven_is_uuid(command_id)) return HAVEN_ERR_INVALID_UUID;

    char pair[128];
    haven_dp single = *value;
    single.dp = dp;
    int written = 0;
    haven_status st = write_dp(&single, pair, sizeof pair, &written);
    if (st != HAVEN_OK) return st;
    (void)written;

    int n = snprintf(out, out_len,
                     "{\"v\":%d,\"ts\":%lld,\"ok\":true,\"id\":\"%s\",\"dp\":\"%s\",\"value\":",
                     HAVEN_WIRE_VERSION, (long long)ts, command_id, dp);
    if (n < 0) return HAVEN_ERR_INVALID_ARG;
    if ((size_t)n >= out_len) return HAVEN_ERR_TOO_SMALL;

    /* The value goes in from `pair`, past the `"dp":` prefix the pair already carries. */
    const char *colon = strchr(pair, ':');
    if (!colon) return HAVEN_ERR_INVALID_ARG;

    int m = snprintf(out + n, out_len - (size_t)n, "%s}", colon + 1);
    return finish(n + m, out_len);
}

haven_status haven_refusal(const char *command_id, int64_t ts, const char *code,
                           const char *message, char *out, size_t out_len) {
    if (!valid(command_id) || !valid(code) || !valid(out)) return HAVEN_ERR_INVALID_ARG;
    if (!haven_is_uuid(command_id)) return HAVEN_ERR_INVALID_UUID;
    if (valid(message) && strchr(message, '"')) return HAVEN_ERR_INVALID_ARG;

    int n = snprintf(out, out_len,
                     "{\"v\":%d,\"ts\":%lld,\"id\":\"%s\",\"ok\":false,\"code\":\"%s\","
                     "\"msg\":\"%s\"}",
                     HAVEN_WIRE_VERSION, (long long)ts, command_id, code,
                     valid(message) ? message : "");
    return finish(n, out_len);
}

haven_status haven_ota_report(const char *campaign_id, const char *state, int pct, int64_t ts,
                              char *out, size_t out_len) {
    if (!valid(campaign_id) || !valid(state) || !valid(out)) return HAVEN_ERR_INVALID_ARG;
    if (!haven_is_uuid(campaign_id)) return HAVEN_ERR_INVALID_UUID;

    /* A percentage outside 0–100 is a firmware bug; clamping it silently would make the
     * campaign progress bar lie. */
    if (pct < 0 || pct > 100) return HAVEN_ERR_INVALID_ARG;

    int n = snprintf(out, out_len, "{\"v\":%d,\"ts\":%lld,\"campaign\":\"%s\",\"state\":\"%s\","
                                   "\"pct\":%d}",
                     HAVEN_WIRE_VERSION, (long long)ts, campaign_id, state, pct);
    return finish(n, out_len);
}

haven_status haven_claim_body(const haven_claim_request *request, char *out, size_t out_len) {
    if (!valid(request) || !valid(out)) return HAVEN_ERR_INVALID_ARG;
    if (!valid(request->product_key) || !valid(request->serial) || !valid(request->secret) ||
        !valid(request->ticket)) {
        return HAVEN_ERR_INVALID_ARG;
    }

    int n = snprintf(out, out_len,
                     "{\"pk\":\"%s\",\"sn\":\"%s\",\"secret\":\"%s\",\"ticket\":\"%s\"%s%s%s}",
                     request->product_key, request->serial, request->secret, request->ticket,
                     valid(request->firmware) ? ",\"fw\":\"" : "",
                     valid(request->firmware) ? request->firmware : "",
                     valid(request->firmware) ? "\"" : "");
    return finish(n, out_len);
}

const char *haven_refusal_unknown_dp(void) { return "unknown_dp"; }
const char *haven_refusal_bad_type(void) { return "bad_type"; }
const char *haven_refusal_out_of_range(void) { return "out_of_range"; }
const char *haven_refusal_not_writable(void) { return "not_writable"; }
