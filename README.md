# Scrollscope by Ardura

Available as a VST3 and CLAP. This is an oscilloscope with a few different features I’ve wanted for myself. So I’m sharing that!

## Example
[![Scrollscope Frequency Analyzer](https://markdown-videos-api.jorgenkh.no/url?url=https%3A%2F%2Fyoutu.be%2Fbsk1fAZlk-k)](https://youtu.be/bsk1fAZlk-k)
![analyzer](https://github.com/ardura/Scrollscope/assets/31751444/bb09c85c-c2c0-425a-a1f5-49dc4c025382)
![scope](https://github.com/ardura/Scrollscope/assets/31751444/255cfc19-5000-49fa-a385-10af79fa7d6a)

Note this can take a sidechain input! Do the routing in FL in plugin processing tab:
![image](https://github.com/ardura/Scrollscope/assets/31751444/6f7c6c75-afa0-47a4-8914-8d1c899ad572)


## Installation
VST3: Copy the vst3 file to C:\Program Files\Common Files\VST3
CLAP: Copy the CLAP file to C:\Program Files\Common Files\CLAP

I don't know the plugin install locations for linux or mac sorry

## Features
- Sidechain input graphing - simply route sidechain input from another channel (up to 5)
- Frequency analysis of multiple channels
- Scaling signals up and down with gain
- Displaying large or small sample sizes
- Optimization with skipping amount configurable
- Reordering waveforms to display main or sidechain on top
- Beat synchronization and Bar Synchronization
- Support for different DAWs with different time-tracking modes (Alt Sync option)

This plugin was made possible thanks to the Nih-Plug Rust Library and the egui GUI library

## Building

After installing [Rust](https://rustup.rs/), you can compile Scrollscope as follows:

```shell
cargo xtask bundle scrollscope --profile release
```
