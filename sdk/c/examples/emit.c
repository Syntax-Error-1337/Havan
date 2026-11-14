/*
 * emit.c — print the envelopes a device would publish, for a given link.
 *
 * Two uses. In CI it is how the C component's output is checked against the server (publish
 * what this prints, read the value back through the API). On a desk it is the quickest way to
 * see what firmware is about to send without wiring a broker into a test.
 *
 *   cc -std=c11 -I. examples/emit.c haven.c -o emit
 *   ./emit 11111111-… 22222222-… 33333333-…
 */
#include <stdio.h>
#include <string.h>

#include "haven.h"

int main(int argc, char **argv) {
    if (argc < 4) {
        fprintf(stderr, "usage: %s <vendor-id> <home-id> <device-id>\n", argv[0]);
        return 2;
    }

    haven_link link;
    snprintf(link.vendor_id, sizeof link.vendor_id, "%s", argv[1]);
    snprintf(link.home_id, sizeof link.home_id, "%s", argv[2]);
    snprintf(link.device_id, sizeof link.device_id, "%s", argv[3]);

    char topic[160];
    char json[HAVEN_ENVELOPE_MAX];

    haven_dp pts[2] = {
        {.dp = "1", .kind = HAVEN_DP_BOOL, .value.b = true},
        {.dp = "2", .kind = HAVEN_DP_INT, .value.i = 80},
    };

    if (haven_topic_shadow(&link, topic, sizeof topic) != HAVEN_OK) {
        fprintf(stderr, "bad link\n");
        return 1;
    }
    if (haven_shadow(pts, 2, 1767225600, true, "1.0.0", json, sizeof json) != HAVEN_OK) {
        fprintf(stderr, "could not build the shadow\n");
        return 1;
    }
    printf("topic=%s\npayload=%s\n", topic, json);

    if (haven_topic_event(&link, topic, sizeof topic) != HAVEN_OK) return 1;
    if (haven_state_event(pts, 2, 1767225601, json, sizeof json) != HAVEN_OK) return 1;
    printf("topic=%s\npayload=%s\n", topic, json);

    return 0;
}
