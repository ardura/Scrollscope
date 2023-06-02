# Scrollscope by Ardura

Available as a VST3 and CLAP

## Installation
VST3: Copy the vst3 file to C:\Program Files\Common Files\VST3
CLAP: Copy the CLAP file to C:\Program Files\Common Files\CLAP

This is an oscilloscope with a few different features I’ve wanted for myself. So I’m sharing that!

## Features
- Sidechain input graphing - simply route sidechain input from another channel
- Zoom focus - Right click and zoom a window! Doubleclick to exit
- Scaling signals up and down with gain
- Displaying large or small sample sizes
- Optimization with skipping amount configurable
- Reordering waveforms to display main or sidechain on top
- Beat synchronization
- Color changes* (Does not save currently)

This plugin was made possible thanks to the Nih-Plug Rust Library and the egui GUI library

## Building

After installing [Rust](https://rustup.rs/), you can compile Scrollscope as follows:

```shell
cargo xtask bundle scrollscope --release
```
