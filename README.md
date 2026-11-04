# cargo-embassy

Create Embassy firmware projects for STM32, Nordic NRF52, and ESP chips from an
interactive terminal or a script.

## Install and run

```sh
cargo install --path . --locked
cargo embassy
# During development:
cargo run -- embassy
```

The TUI has three steps: project/chip selection, categorized options, and a
review of the files to generate. The terminal must be at least 60 × 20.

- Type to edit the project name, chip filter, output parent, or toolchain.
- Tab / Shift-Tab changes fields; Up / Down selects a chip or menu row.
- In the toolchain field, Left / Right cycles the locally installed toolchains.
- Enter opens a category; Space toggles an option. Esc goes back.
- Dependencies are selected automatically. Mutually exclusive options replace
  one another; removing a dependency clears dependent options.
- The help panel describes requirements and selections that will be cleared.
- F2 previews the files. Enter on the review page writes the project.
- Ctrl-C cancels without writing; Esc on the first page also cancels.

Generation uses a staged directory and refuses existing destinations. It does
not run Cargo dependency installation, install toolchains, initialize Git,
access a probe, or flash hardware. `rustfmt` must be installed; generated Rust
is formatted before the destination is created. Install target toolchains and
flash/debug tools when you are ready to build and run the resulting firmware.

## Command line

```sh
cargo embassy init hello --chip stm32f103c6 --logging defmt --headless
cargo embassy init nordic --chip nrf52840 --logging log -o alloc -o ci -o vscode
cargo embassy init wireless --chip esp32c6 -o wifi -o ble-trouble -o defmt -o wokwi -o ci
cargo embassy init debug-esp --chip esp32s3 -o probe-rs -o defmt -o panic-rtt-target -o embedded-test
cargo embassy init ble-nrf --chip nrf52832-xxaa --softdevice s132
cargo embassy init quiet --chip esp32c3 --logging none
```

Use `cargo embassy tui --chip esp32c6 --dry-run` to explicitly open the TUI
with presets while preventing writes. Put generation flags after `init` or
`tui` when using a subcommand; they belong to that command.

The `init` keyword is optional. Missing name/chip inputs open the TUI unless
`--headless` was passed. Fully specified input stays noninteractive.

`-o / --option` is repeatable. `--logging auto|defmt|log|none`, `--vscode`,
`--softdevice`, and `--panic-handler` are convenient aliases. The default
frontend is defmt for Cortex-M and log for ESP. Deselect the active logging
option in the TUI to disable application logging; panic output is independent.
Explicitly requesting conflicting options on the CLI is an error.

```sh
cargo embassy list-chips stm32g0
cargo embassy list-options --chip esp32c6
cargo embassy explain wifi --chip esp32c6
cargo embassy check --chip esp32c6
cargo embassy init preview --chip esp32c6 -o wifi --dry-run
cargo embassy init example --chip stm32f103c6 --output-path /existing/parent --toolchain stable
```

`check` renders each applicable option independently without writing files.
`--dry-run` validates the selected combination and lists its output files.
`--output-path` names an existing **parent directory**; the project is created
inside it using the project name. Toolchain auto selects stable, esp for Xtensa,
or nightly for stack protection. An explicit toolchain is not installed for you.

## Features and comparison with esp-generate

The CLI, TUI, configuration model, and output pipeline are rewritten. ESP
options, compatibility constraints, and templates are embedded through the
`esp-generate` Rust library. **No external `esp-generate` executable is used.**
The dependency version in `Cargo.lock` identifies the template release.
STM32/NRF generation uses the same selection model and a separate Cortex-M
renderer. This is not a claim that ESP-only hardware exists on other MCUs.

| Capability | ESP | STM32 / NRF52 |
| --- | --- | --- |
| Chip selection | All chips from the embedded ESP template release | Bundled probe-rs STM32 / NRF52 database |
| Module / flash / PSRAM presets | Upstream module choices and constraints | Board-specific; no invented module presets |
| Embassy async runtime | esp-rtos + Embassy | Cortex-M executor + family HAL |
| Logging | defmt / log / none; backend follows runner | defmt / log / none over RTT |
| Panic | esp-backtrace / panic-rtt-target / template fallback | halt / reset / panic-rtt-target |
| Heap allocation | esp-alloc | embedded-alloc, RAM-aware heap size |
| Wi-Fi | esp-radio on compatible chips | No built-in generic Wi-Fi hardware |
| BLE | esp-radio + trouble on compatible chips | NRF Softdevice GATT peripheral or S122 scanner |
| Flash/debug runner | espflash or probe-rs | probe-rs |
| cargo-embed | Optional Embed.toml | Optional Embed.toml |
| Embedded tests | Upstream embedded-test example | Standalone embedded-test smoke target |
| Stack protection | Upstream template, nightly / esp | Nightly stack checks and failure handler |
| Wokwi | Supported upstream chip diagrams and config | No generic board diagram fabricated |
| CI | GitHub Actions template | Build / fmt / clippy workflow |
| Editors | VS Code, Helix, Zed, both Neovim integrations | Same editor choices |
| Agent guidance | AGENTS / CLAUDE / CODEX / GEMINI | Same mutually exclusive choices |
| Toolchain | Auto / installed selection / explicit name | Auto / installed selection / explicit name |
| Help / preview | Categorized options, dependencies, file review | Same |

ESP radio templates initialize the controller/stack; network credentials,
application protocols, and board-specific wiring remain application work, as
in esp-generate. Nordic Softdevices must be flashed separately. Their generated
`memory.x` reservations must be checked against the selected Softdevice release
and BLE configuration before running hardware. Generation does not perform
that hardware acceptance check.

Generated Cortex-M examples avoid assigning arbitrary LED pins. Choose GPIOs
using your board schematic. Device names absent from a HAL release may still
exist in the probe-rs database; generation validation is not a firmware build.

## Development verification

```sh
./ci.sh
# Optional render sweep for ESP templates:
./ci-esp.sh
```

Tests cover CLI parsing, requirement cascades, conflicts, missing logging,
ESP feature combinations, chip/option render sweeps, Rust syntax, TOML/JSONC,
and destination protection. These are native host checks. They do not launch
or automate the TUI, run embedded tests on a board, or prove generated firmware
against every target toolchain.
