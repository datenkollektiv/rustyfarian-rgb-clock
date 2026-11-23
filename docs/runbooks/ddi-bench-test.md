# Runbook: DDI Bench Test

Run this after any change to the DDI client (`src/ota/`, the `ddi-lab` feature, or the DDI runtime in `rustyfarian-esp-idf-network`), and before a lab image goes to the fleet.
It enrols one lab board with a DDI server, forces an update, forces an unhealthy image to roll back, and measures the time from admission to close.
Record results in the PR, not here.
This runbook has not been run on hardware yet; treat each step as a procedure to follow.

## Prerequisites

- A lab DDI server: hawkBit 1.2.0, or a server with the same Management API subset, reachable from the board on the LAN.
- The upstream operator recipes (`just ddi-*`), run from the sibling checkout `../rustyfarian-network`.
  They read their settings from the shell or that checkout's `.env`.
- Host tools: `just`, `espflash`, `jq`, `curl`, and `mosquitto_pub` for the nudge.
- The broker from `.env` (`MQTT_HOST`, `MQTT_PORT`) is reachable from the board and from this machine.
- A board with the WS2812 ring on the standard pin (GPIO18 on the C6, GPIO4 on the C3).
- A device token from the fleet operator: 32 to 64 lowercase hex characters, one per device.
  Keep it out of committed files and out of logs.

The device id is `rgb-clock-<mac4>`, where `<mac4>` is the last four hex digits of the board's MAC address.
Read the MAC address with `espflash board-info`, and use the same id everywhere below.

### Base URL

The board reaches the server through `DDI_BASE_URL`, compiled in at build time.
Point it either at the lab server directly, or at the lab proxy from `just ddi-proxy` (default port 8090).
The proxy is optional; it throttles artifacts so that the cancel and reset cases have a window.
Export it once in the `rgb-clock` shell, and the build and bundle recipes below read it from there.

```sh
export DDI_BASE_URL=http://<lab-host>:<port>/DEFAULT
```

### Operator shell

Set these in the shell that runs the `ddi-*` recipes, from the network checkout.
Replace each placeholder with your own value, and do not write the password or token into a file.

```sh
export DDI_MGMT_URL=http://<lab-host>:<port>
export DDI_MGMT_USER=<operator-user>
export DDI_MGMT_PASS=<operator-password>
export DDI_CONTROLLER_ID=rgb-clock-<mac4>
export DDI_TOKEN=<token-from-the-fleet-operator>
export MQTT_HOST=<broker-host>
```

If the broker needs credentials, also export `MQTT_PORT`, `MQTT_USER` and `MQTT_PASS`.

## Lab server setup

Run these once per lab server, and again after a server reset.
Both run from the network checkout.

```sh
just ddi-tenant-init
just ddi-ensure-target
```

`ddi-tenant-init` enables target-token authentication and sets the 30 s polling interval.
A server without those tenant keys answers 400, and the recipe reports that and continues.
`ddi-ensure-target` creates the device's target with `DDI_TOKEN`, or sets that token on an existing target.

## Build and flash the baseline

Erase first only if the board ran an MQTT OTA image, because it must move over by serial flash, erase and re-provisioning.
`just erase-flash` asks for confirmation and clears the stored Wi-Fi and MQTT settings.

```sh
just erase-flash
```

Build and flash the lab image from the `rgb-clock` checkout.
The baseline version is the `version` in `Cargo.toml`.

```sh
just flash-lab
```

Start the serial capture in a second shell, after the flash finished, and keep it running for the whole test.
The capture is the source for every marker below and for the measurement.

```sh
just monitor-log tmp/ddi-bench-run.log
```

Check that the capture is still growing after a reset, and that `pgrep -fl espflash` shows the monitor.
If the capture stops with `Broken pipe`, restart it and read the missing lines from the device again.

## Enrol through the portal

A `ddi-lab` image without a valid token opens the provisioning portal instead of running the clock.

1. Join the open `Rustyfarian-XXXX` access point and open the captive portal.
2. Fill in the Wi-Fi and MQTT fields as usual.
3. Enter the token in the `ddi_token` field.
4. Save.
   The board restarts, and with a valid token it runs the clock.

Within about 30 s of the Wi-Fi link coming up, the capture should show the first poll and the configuration send:

- `DDI poll answered: Ok`
- `DDI configData sent`

## Forced update

This step offers a newer version, the bundle for `0.3.1`, and expects the board to install it.
Choose a version above the baseline; `0.3.1` assumes the baseline is `0.3.0`.

Build the bundle from the `rgb-clock` checkout:

```sh
just ddi-bundle idf_c6_rgb_clock 0.3.1
```

The upstream helper looks for the bundle under the device id in the network checkout.
Run this copy from the `rgb-clock` checkout, in the shell where `DDI_CONTROLLER_ID` is exported, to place the four bundle files there with the device id in the name.

```sh
mkdir -p ../rustyfarian-network/tmp/ddi-bundle
for ext in bin bin.sha256 bin.version bin.chip; do
  cp "tmp/ddi-bundle/idf_c6_rgb_clock-v0.3.1.$ext" "../rustyfarian-network/tmp/ddi-bundle/idf_c6_rgb_clock-$DDI_CONTROLLER_ID-v0.3.1.$ext"
done
```

From the network checkout, publish the set and assign it to the device with the nudge:

```sh
just ddi-publish idf_c6_rgb_clock 0.3.1
just ddi-assign idf_c6_rgb_clock 0.3.1 --nudge
```

`ddi-assign` prints `action=<id>`.
Keep that id for the rest of the step.

Expect these lines in the capture, in this order, for that action id:

1. `[ota] deployment <id> admitted: version 0.3.1, <n> bytes`
2. `[ota] attempt <n> begun for action <id>`
3. `DDI download from http://`
4. `[ota] action <id>: boot slot selected; restarting in <ms> ms`
5. The board boots the new image.
6. `[ota] image 0.3.1 is healthy; the slot is marked valid`
7. `[ota] verdict for action <id> delivered; the action is closed`

Then confirm the server side from the network checkout:

```sh
just ddi-watch <id>
```

The probe line should reach `status=finished code=none detail=none`.
Stop the probe with Ctrl-C when the step is done.

## Unhealthy rollback

The `unhealthy` demo feature implies `ddi-lab`, keeps the health policy, and sets the failure deadline to 15 s, so the rollback comes quickly.

Build the unhealthy bundle as `0.3.2` from the `rgb-clock` checkout; `DDI_VARIANT=unhealthy` selects the demo image and names the bundle `idf_c6_rgb_clock-unhealthy-v0.3.2`:

```sh
DDI_VARIANT=unhealthy just ddi-bundle idf_c6_rgb_clock 0.3.2
```

Copy it under the device id, without the `unhealthy` part, so the upstream helper finds it as version `0.3.2`:

```sh
for ext in bin bin.sha256 bin.version bin.chip; do
  cp "tmp/ddi-bundle/idf_c6_rgb_clock-unhealthy-v0.3.2.$ext" "../rustyfarian-network/tmp/ddi-bundle/idf_c6_rgb_clock-$DDI_CONTROLLER_ID-v0.3.2.$ext"
done
```

From the network checkout, publish and assign it with the nudge:

```sh
just ddi-publish idf_c6_rgb_clock 0.3.2
just ddi-assign idf_c6_rgb_clock 0.3.2 --nudge
```

Expect these lines for that action id:

1. `[ota] deployment <id> admitted: version 0.3.2`
2. The board boots `0.3.2`, never turns healthy, and reaches the 15 s deadline.
3. `[ota] health deadline reached with the slot still pending; rolling back`
4. The board boots `0.3.1` again.
5. `[ota] verdict for action <id> delivered; the action is closed`

`just ddi-watch <id>` should reach `status=error code=1004 detail=rolled_back`.

Offer `0.3.2` again.
The server must refuse it before any download:

```sh
just ddi-assign idf_c6_rgb_clock 0.3.2 --nudge
```

Expect `[ota] deployment <id> refused: Policy previously_rolled_back`, and a probe result of `code=1007 detail=previously_rolled_back`.

## Measure the admitted-to-closed interval

The interval runs from the `admitted` line of the forced update to the `closed` line of the same action id.
It covers the download, the reboot, the 30 s health dwell after boot, and the verdict delivery.

Find the two lines in the capture, with the action id from the forced update:

```sh
grep -E "deployment <id> admitted|verdict for action <id> delivered" tmp/ddi-bench-run.log
```

If the lines carry the ESP-IDF timestamp prefix `I (<ms>)`, the interval is the difference of the two millisecond values.
If they carry no timestamp, time the run with a stopwatch from the moment each line appears, and write that method down next to the result.

Record the interval, the version pair, the board, and the date in the PR.

Also record the NVS line the firmware prints, which reports the entries used, free and total with the token and the DDI action record stored.
The feature doc asks what this line reports on the C6, so note its exact text.

## Troubleshooting

| Symptom | Likely cause | Action |
|:--------|:-------------|:-------|
| Build fails in `build.rs` and names `DDI_BASE_URL` | A `ddi-lab` build without `DDI_BASE_URL`, or with a blank value | Set `DDI_BASE_URL` in the shell or `.env`, then rebuild |
| The portal opens after a `ddi-lab` flash | `ddi_token` is missing or is not 32 to 64 lowercase hex characters | Enter the token from the fleet operator and save |
| `DDI poll answered: Unauthorized` | The device token does not match the target, or target-token auth is off | Re-run `just ddi-ensure-target` with the same `DDI_TOKEN`, and `just ddi-tenant-init` |
| `DDI poll answered: NotFound` | No target exists for the device id | Set `DDI_CONTROLLER_ID` to the device id, then run `just ddi-ensure-target` |
| `DDI base URL refused` | An `https://` base URL, which the client does not support yet | Use `http://` in a `ddi-lab` build; TLS is a later round |
| `ddi-assign` says the bundle is missing | The bundle was not copied under the device id in the network checkout | Repeat the copy step, or check the file names |
| `ddi-assign` says no action was created | The same set is already installed or running on the target | Use a new version, or run `just ddi-target-reset` to start the target over |
| `module ... holds an artifact with sha256 ...` | A rebuilt image was given a version that was already uploaded | Build and publish under a new version |
| `download of action <id> failed: checksum_mismatch` | The uploaded bytes differ from the bundle's `.bin.sha256` | Rebuild the bundle, then publish it under a new version |
| `refused: Policy up_to_date`, `target_mismatch` or `previously_rolled_back` | The server refused the offer on purpose | Offer a new version for the chip; this is the expected result for the refusal steps |
| The nudge is sent but the board does not poll early | The board is not on the broker, or `MQTT_HOST` is wrong | Check the broker and `MQTT_HOST`; the board still polls within 30 s |
