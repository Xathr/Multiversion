# Multiversion — every 1.21+ player on one Pumpkin server

**Multiversion** lets players on **Minecraft Java 1.21 through 26.3** join your Pumpkin server. Your friends no longer have to update, downgrade or install anything. It runs entirely inside the server as a WebAssembly plugin: **no proxy, no Java, no second process, no extra port.**

![Supported versions](https://raw.githubusercontent.com/jayclrk358/Multiversion/main/market/screenshot-versions.png)

## Supported versions

| Client | Status |
|---|---|
| 26.3 | native |
| 26.2, 26.1 | translated |
| 1.21.11, 1.21.9–1.21.10, 1.21.7–1.21.8 | translated |
| 1.21.6, 1.21.5, 1.21.4 | translated |
| 1.21.2–1.21.3, 1.21–1.21.1 | translated |

## Features

- 🔁 **Full protocol translation**: chunks, blocks, items with all their data, entities, inventories, recipes, chat, sounds, particles and commands are converted packet by packet, version step by version step
- 🔐 **Real online mode**: players are verified with Mojang, and their real UUID, skin and IP reach Pumpkin through signed Velocity-style forwarding
- 🎛️ **You choose who joins**: allow every version, a family like `1.21.*`, or a range like `1.21.5-26.3`, and deny specific ones
- 🧾 **Clear console logs**: every join and leave shows the player's version and address, including Bedrock players' version and device
- 🧱 **Sensible stand-ins**: blocks, items and mobs an older client doesn't have appear as their closest match
- 🪶 **Self-contained**: one `.wasm` file and one config file

![Console](https://raw.githubusercontent.com/jayclrk358/Multiversion/main/market/screenshot-console.png)

## Setup

1. Put `multiversion.wasm` in your server's `plugins/` folder and start the server once.
2. The plugin writes `plugins/data/multiversion/pumpkin-settings.txt`: three settings to paste into `pumpkin.toml`. They move Pumpkin to a local port, turn on forwarding with a generated secret, and pre-approve the plugin's permissions. Plugins can only write their own folder, so this one step is manual.
3. Restart. Players keep connecting to port 25565 as before. Keep port 25566 closed in your firewall.

If a setting is missing, the plugin says exactly which one in the console.

## Configuration

`plugins/data/multiversion/config.toml`:

```toml
[versions]
allow = ["*"]   # or e.g. ["1.21.*", "26.3"]
deny = []       # e.g. ["1.21"]
```

![Version filter](https://raw.githubusercontent.com/jayclrk358/Multiversion/main/market/screenshot-config.png)

You can also change the public and backend ports, online mode, the server-list version text, the kick message, compression and connection limits.

## Permissions it asks for

| Permission | Why |
|---|---|
| `network.tcp` | Listen on the public port and connect to Pumpkin on localhost |
| `http.outbound` | Verify players with Mojang's session server |
| `fs.read.data` | Read its config |
| `fs.write.data` | Write its default config and the settings snippet |

## Good to know

- Content newer than a player's version appears as the closest older equivalent. For example, copper golems show as frogs on 1.21.8 and older, and happy ghasts show as ghasts on 1.21.5.
- 1.21.5 and older clients can't show dialog screens or locator-bar waypoints, so those are skipped for them.
- Built for Pumpkin 26.3 (plugin API v0.1).

Open source under GPL-3.0. Please report any disconnect message on GitHub so it can be fixed.
