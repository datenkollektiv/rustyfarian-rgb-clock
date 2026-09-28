#!/usr/bin/env bash
set -euo pipefail
# ota-mqtt.sh — publish to or subscribe on the OTA topics via the broker in MQTT_HOST/MQTT_PORT
# Usage:
#   scripts/ota-mqtt.sh pub <topic> <message>
#   scripts/ota-mqtt.sh sub <topic>
# MQTT_HOST is required (the justfile loads it from .env); MQTT_PORT defaults to 1883.
# Uses the mqttx CLI when installed, else mosquitto_pub/mosquitto_sub.
# DRY_RUN=1 prints the command instead of running it.

mode="${1:-}"
topic="${2:-}"
if [ -z "$mode" ] || [ -z "$topic" ] || { [ "$mode" = "pub" ] && [ $# -lt 3 ]; }; then
    printf 'Usage: %s pub <topic> <message> | sub <topic>\n' "$0" >&2
    exit 2
fi
if [ -z "${MQTT_HOST:-}" ]; then
    printf 'Error: MQTT_HOST is not set; add it to .env (see .env.example).\n' >&2
    exit 1
fi
port="${MQTT_PORT:-1883}"

if command -v mqttx >/dev/null 2>&1; then
    case "$mode" in
        pub) cmd=(mqttx pub -h "$MQTT_HOST" -p "$port" -t "$topic" -m "$3") ;;
        sub) cmd=(mqttx sub -h "$MQTT_HOST" -p "$port" -t "$topic") ;;
        *) printf 'Error: mode must be "pub" or "sub", got "%s"\n' "$mode" >&2; exit 2 ;;
    esac
elif command -v mosquitto_pub >/dev/null 2>&1; then
    case "$mode" in
        pub) cmd=(mosquitto_pub -h "$MQTT_HOST" -p "$port" -t "$topic" -m "$3") ;;
        sub) cmd=(mosquitto_sub -h "$MQTT_HOST" -p "$port" -t "$topic" -v) ;;
        *) printf 'Error: mode must be "pub" or "sub", got "%s"\n' "$mode" >&2; exit 2 ;;
    esac
else
    printf 'Error: no MQTT client found; install the mqttx CLI or mosquitto.\n' >&2
    exit 1
fi

if [ "${DRY_RUN:-0}" = "1" ]; then
    printf '[dry-run] would run:'
    printf ' %q' "${cmd[@]}"
    printf '\n'
    exit 0
fi
exec "${cmd[@]}"
