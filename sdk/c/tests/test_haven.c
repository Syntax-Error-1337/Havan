/*
 * Tests for the C component. Compile and run with `make -C sdk/c test`.
 *
 * No framework: a firmware vendor will not add one, and the properties worth testing are
 * string shapes — an assertion macro and a main are enough, and they keep the component's
 * dependencies at zero.
 */
#include <stdio.h>
#include <string.h>

#include "../haven.h"

static int failures = 0;
static int checks = 0;

#define CHECK(cond)                                                                    \
    do {                                                                               \
        checks++;                                                                      \
        if (!(cond)) {                                                                 \
            failures++;                                                                \
            printf("  FAIL %s:%d  %s\n", __FILE__, __LINE__, #cond);                   \
        }                                                                              \
    } while (0)

#define CHECK_STR(actual, expected)                                                    \
    do {                                                                               \
        checks++;                                                                      \
        if (strcmp((actual), (expected)) != 0) {                                        \
            failures++;                                                                \
            printf("  FAIL %s:%d\n    expected: %s\n    actual:   %s\n", __FILE__,      \
                   __LINE__, (expected), (actual));                                    \
        }                                                                              \
    } while (0)

static haven_link link(void) {
    haven_link l;
    snprintf(l.vendor_id, sizeof l.vendor_id, "11111111-1111-7111-8111-111111111111");
    snprintf(l.home_id, sizeof l.home_id, "22222222-2222-7222-8222-222222222222");
    snprintf(l.device_id, sizeof l.device_id, "33333333-3333-7333-8333-333333333333");
    return l;
}

static void topics_are_built_not_formatted(void) {
    char out[128];
    haven_link l = link();

    CHECK(haven_topic_event(&l, out, sizeof out) == HAVEN_OK);
    CHECK_STR(out, "haven/v1/11111111-1111-7111-8111-111111111111/"
                   "22222222-2222-7222-8222-222222222222/"
                   "33333333-3333-7333-8333-333333333333/evt");

    CHECK(haven_topic_shadow(&l, out, sizeof out) == HAVEN_OK);
    CHECK(strstr(out, "/shadow") != NULL);
    CHECK(haven_topic_ack(&l, out, sizeof out) == HAVEN_OK);
    CHECK(strstr(out, "/ack") != NULL);
    CHECK(haven_topic_ota(&l, out, sizeof out) == HAVEN_OK);
    CHECK(strstr(out, "/ota") != NULL);
    CHECK(haven_topic_cmd(&l, out, sizeof out) == HAVEN_OK);
    CHECK(strstr(out, "/cmd") != NULL);
}

static void a_bad_id_is_refused_not_published(void) {
    char out[128];
    haven_link l = link();

    /* A truncated id would build a topic nobody is subscribed to: worse than an error. */
    snprintf(l.device_id, sizeof l.device_id, "33333333-3333-7333-8333");
    CHECK(haven_topic_event(&l, out, sizeof out) == HAVEN_ERR_INVALID_UUID);

    snprintf(l.device_id, sizeof l.device_id, "not-a-uuid-at-all-not-a-uuid-at-all-1234");
    CHECK(haven_topic_event(&l, out, sizeof out) == HAVEN_ERR_INVALID_UUID);
}

static void a_small_buffer_reports_truncation(void) {
    char small[24];
    haven_link l = link();

    /* The point: it must not write a prefix of the topic and claim success. */
    CHECK(haven_topic_event(&l, small, sizeof small) == HAVEN_ERR_TOO_SMALL);
}

static void values_keep_their_type(void) {
    char out[256];
    haven_dp pts[3];

    pts[0] = (haven_dp){.dp = "1", .kind = HAVEN_DP_BOOL, .value.b = true};
    pts[1] = (haven_dp){.dp = "2", .kind = HAVEN_DP_INT, .value.i = 80};
    pts[2] = (haven_dp){.dp = "3", .kind = HAVEN_DP_TEXT, .value.text = "#ff0000"};

    CHECK(haven_state_event(pts, 3, 1700000000, out, sizeof out) == HAVEN_OK);
    CHECK(strstr(out, "\"1\":true") != NULL);
    CHECK(strstr(out, "\"2\":80") != NULL);
    CHECK(strstr(out, "\"3\":\"#ff0000\"") != NULL);
    CHECK(strstr(out, "\"v\":1") != NULL);
    CHECK(strstr(out, "\"ts\":1700000000") != NULL);
}

static void a_float_keeps_its_precision(void) {
    char out[128];
    haven_dp p = {.dp = "4", .kind = HAVEN_DP_FLOAT, .value.f = 21.4567};

    CHECK(haven_state_event(&p, 1, 1, out, sizeof out) == HAVEN_OK);
    /* %.6g: enough that a temperature does not drift a decimal every report. */
    CHECK(strstr(out, "21.4567") != NULL);
}

static void text_with_a_quote_is_a_caller_mistake(void) {
    char out[128];
    haven_dp p = {.dp = "5", .kind = HAVEN_DP_TEXT, .value.text = "a\"b"};

    /* Refused rather than escaped: firmware that passes a quote has a bug, and hiding it
     * would put invalid JSON on the wire. */
    CHECK(haven_state_event(&p, 1, 1, out, sizeof out) == HAVEN_ERR_INVALID_ARG);
}

static void the_shadow_carries_presence_and_firmware(void) {
    char out[256];
    haven_dp p = {.dp = "1", .kind = HAVEN_DP_BOOL, .value.b = false};

    CHECK(haven_shadow(&p, 1, 1700000000, true, "1.0.0", out, sizeof out) == HAVEN_OK);
    CHECK(strstr(out, "\"online\":true") != NULL);
    CHECK(strstr(out, "\"fw\":\"1.0.0\"") != NULL);
}

static void an_ack_names_the_command_and_the_value(void) {
    char out[256];
    haven_dp value = {.dp = "1", .kind = HAVEN_DP_BOOL, .value.b = true};

    CHECK(haven_ack("01a1d8e2-6218-75d6-869c-272cb8b37c95", "1", &value, 1700000000, out,
                    sizeof out) == HAVEN_OK);
    CHECK(strstr(out, "\"id\":\"01a1d8e2-6218-75d6-869c-272cb8b37c95\"") != NULL);
    CHECK(strstr(out, "\"dp\":\"1\"") != NULL);
    /* AckMsg.ok is not optional on the server: an ack without it is rejected as malformed, which
     * is how a device that applied a command still looks like one that ignored it. */
    CHECK(strstr(out, "\"ok\":true") != NULL);
    /* The value must appear once, from the typed writer — not as a quoted string. */
    CHECK(strstr(out, "\"value\":true") != NULL);
    CHECK(strstr(out, "\"value\":\"") == NULL);
}

static void a_refusal_is_typed(void) {
    char out[256];

    CHECK(haven_refusal("01a1d8e2-6218-75d6-869c-272cb8b37c95", 1700000000,
                        haven_refusal_unknown_dp(), "no such datapoint", out, sizeof out) ==
          HAVEN_OK);
    CHECK(strstr(out, "\"ok\":false") != NULL);
    CHECK(strstr(out, "\"code\":\"unknown_dp\"") != NULL);
    CHECK(strstr(out, "\"msg\":\"no such datapoint\"") != NULL);
}

static void ota_progress_rejects_an_impossible_percentage(void) {
    char out[256];

    CHECK(haven_ota_report("01a1d8e2-6218-75d6-869c-272cb8b37c95", "downloading", 40, 1, out,
                           sizeof out) == HAVEN_OK);
    CHECK(strstr(out, "\"pct\":40") != NULL);

    /* Clamping would make the portal's progress bar lie about a bricked flash. */
    CHECK(haven_ota_report("01a1d8e2-6218-75d6-869c-272cb8b37c95", "downloading", 120, 1, out,
                           sizeof out) == HAVEN_ERR_INVALID_ARG);
}

static void the_claim_body_is_what_the_server_parses(void) {
    char out[512];
    haven_claim_request request = {
        .product_key = "hv_acme_bulb1",
        .serial = "SN-1",
        .secret = "s3cret",
        .ticket = "hv1.abc",
        .firmware = "1.0.0",
    };

    CHECK(haven_claim_body(&request, out, sizeof out) == HAVEN_OK);
    CHECK(strstr(out, "\"pk\":\"hv_acme_bulb1\"") != NULL);
    CHECK(strstr(out, "\"sn\":\"SN-1\"") != NULL);
    CHECK(strstr(out, "\"ticket\":\"hv1.abc\"") != NULL);
    CHECK(strstr(out, "\"fw\":\"1.0.0\"") != NULL);

    /* Firmware is optional; the body must still be valid JSON without it. */
    request.firmware = NULL;
    CHECK(haven_claim_body(&request, out, sizeof out) == HAVEN_OK);
    CHECK(strstr(out, "\"fw\"") == NULL);
    CHECK(out[strlen(out) - 1] == '}');
}

/* Print the three envelopes the device publishes, so a test in another language can parse them.
 *
 * This exists because the C SDK and the Rust server disagreed about the wire format for as long as
 * nothing compared them: its shadow said `on` where the server requires `online`, and its ack
 * omitted `ok`. Both passed 45 checks that only compared the SDK against itself.
 */
static int emit(void) {
    char out[256];
    haven_link link = {"11111111-1111-7111-8111-111111111111",
                       "22222222-2222-7222-8222-222222222222",
                       "33333333-3333-7333-8333-333333333333"};
    haven_dp pts[2] = {{"1", HAVEN_DP_BOOL, {.b = true}}, {"2", HAVEN_DP_INT, {.i = 80}}};

    if (haven_topic_event(&link, out, sizeof out) != HAVEN_OK) return 1;
    printf("topic %s\n", out);
    if (haven_topic_shadow(&link, out, sizeof out) != HAVEN_OK) return 1;
    printf("topic %s\n", out);
    if (haven_topic_ack(&link, out, sizeof out) != HAVEN_OK) return 1;
    printf("topic %s\n", out);
    if (haven_topic_cmd(&link, out, sizeof out) != HAVEN_OK) return 1;
    printf("topic %s\n", out);

    if (haven_shadow(pts, 2, 1700000000, true, "1.0.0", out, sizeof out) != HAVEN_OK) return 1;
    printf("shadow %s\n", out);
    if (haven_state_event(pts, 2, 1700000000, out, sizeof out) != HAVEN_OK) return 1;
    printf("event %s\n", out);
    if (haven_ack("01a1d8e2-6218-75d6-869c-272cb8b37c95", "1", &pts[0], 1700000000, out,
                  sizeof out) != HAVEN_OK) {
        return 1;
    }
    printf("ack %s\n", out);
    if (haven_refusal("01a1d8e2-6218-75d6-869c-272cb8b37c95", 1700000000, "bad_type",
                      "expected a bool", out, sizeof out) != HAVEN_OK) {
        return 1;
    }
    printf("refusal %s\n", out);
    return 0;
}

int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "--emit") == 0) return emit();

    topics_are_built_not_formatted();
    a_bad_id_is_refused_not_published();
    a_small_buffer_reports_truncation();
    values_keep_their_type();
    a_float_keeps_its_precision();
    text_with_a_quote_is_a_caller_mistake();
    the_shadow_carries_presence_and_firmware();
    an_ack_names_the_command_and_the_value();
    a_refusal_is_typed();
    ota_progress_rejects_an_impossible_percentage();
    the_claim_body_is_what_the_server_parses();

    if (failures > 0) {
        printf("%d of %d checks failed\n", failures, checks);
        return 1;
    }
    printf("all %d checks passed\n", checks);
    return 0;
}
