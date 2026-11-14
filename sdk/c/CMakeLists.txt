# ESP-IDF component wrapper.
#
# Drop this directory into a project's `components/` and `idf.py build` picks it up; nothing
# here needs configuration. ESP-IDF's build system is CMake, so this file is the whole
# integration — the transport (esp_mqtt_client, esp_http_client) belongs to the application,
# which is the point: the component builds bytes, the vendor moves them.
#
# Verified on a host with clang (`make test`); not yet compiled with the xtensa toolchain,
# because no board is attached. The code is C11 with no platform headers, so a toolchain
# failure would be a build-system problem, not a source problem.
idf_component_register(
    SRCS "haven.c"
    INCLUDE_DIRS "."
)
