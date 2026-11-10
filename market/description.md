# Multiversion — every 1.20+ player on one Pumpkin server

**Multiversion** lets players on **Minecraft Java 1.19 through 26.3** join your Pumpkin server. Your friends no longer have to update, downgrade or install anything. It runs entirely inside the server as a WebAssembly plugin: **no proxy, no Java, no second process, no extra port.**

![Supported versions](https://raw.githubusercontent.com/Xathr/Multiversion/main/market/screenshot-versions.png)

## Supported versions

| Client | Status |
|---|---|
| 26.3 | native |
| 26.2, 26.1 | translated |
| 1.21.11, 1.21.9–1.21.10, 1.21.7–1.21.8 | translated |
| 1.21.6, 1.21.5, 1.21.4 | translated |
| 1.21.2–1.21.3, 1.21–1.21.1 | translated |
| 1.20.5–1.20.6, 1.20.3–1.20.4 | translated |
| 1.20.2, 1.20–1.20.1 | translated |
| 1.19.4, 1.19.3 | translated |
| 1.19.1–1.19.2, 1.19 | translated, chat unsigned |

## Features

- 🔁 **Full protocol translation**: chunks, blocks, items with all their data, entities, inventories, recipes, chat, sounds, particles and commands are converted packet by packet, version step by version step
- 🔐 **Real online mode**: players are verified with Mojang, and their real UUID, skin and IP reach Pumpkin through signed Velocity-style forwarding
- 🎛️ **You choose who joins**: allow every version, a family like `1.21.*`, or a range like `1.21.5-26.3`, and deny specific ones
- 🧾 **Clear console logs**: every join and leave shows the player's version and address, including Bedrock players' version and device
- 🧱 **Sensible stand-ins**: blocks, items and mobs an older client doesn't have appear as their closest match
- 🖥️ **Clear startup banner**: the supported versions, ports and login mode at a glance, and every console line tagged `[Multiversion]`
- 🛡️ **Built-in protection**: oversized packets before login are refused, idle logins time out, one address can only hold a few connections that are still logging in and open a limited number per 10 seconds, and malformed data is refused before it can use up memory
- 🔔 **Update check**: tells the console and operators when a newer version is on the Market, and `/multiversion update download` fetches the signed file after checking its signature
- ⚡ **Light on the CPU**: server-list pings are answered from a one-second cache, and Pumpkin's link to the plugin can skip compression
- 🪶 **Self-contained**: one `.wasm` file (about 2.9 MB) and one config file

![Console](https://raw.githubusercontent.com/Xathr/Multiversion/main/market/screenshot-console.png)

## Setup

1. Put `Multiversion.wasm` in your server's `plugins/` folder and start the server.
2. The console lists the four lines to change in `pumpkin.toml`, each with the heading it sits under. The same list is saved as `plugins/data/multiversion/pumpkin-settings.txt`.
3. Stop the server, change those lines in `pumpkin.toml`, and start it again. The console prints `setup check passed`, or lists only the lines that are still wrong.

It is the same on Linux and Windows. Players keep connecting to port 25565 as before; keep port 25566 closed in your firewall. Plugins can only write their own folder, which is why the `pumpkin.toml` edit is yours to make.

## Configuration

`plugins/data/multiversion/config.toml`:

```toml
[backend]
pumpkin_compression = false   # true keeps Pumpkin compressing its local link (costs CPU)

[versions]
allow = ["*"]   # or e.g. ["1.21.*", "26.3"]
deny = []       # e.g. ["1.21"]

[limits]
max_logins_per_ip = 5         # connections one address may have still logging in; 0 = no limit
max_connections_per_ip_per_10s = 40   # new connections one address may open per 10 s; 0 = no limit

[updates]
check = true                  # ask the Pumpkin Market for the latest version at startup
notify_ops = true             # tell operators in game when a newer version is out
download = false              # download a newer, signed Multiversion.wasm at startup
```

![Version filter](https://raw.githubusercontent.com/Xathr/Multiversion/main/market/screenshot-config.png)

You can also change the public and backend ports, online mode, the server-list version text, the kick message, compression and connection limits.

## Permissions it asks for

| Permission | Why |
|---|---|
| `network.tcp` | Listen on the public port and connect to Pumpkin on localhost |
| `http.outbound` | Verify players with Mojang's session server, and ask the Pumpkin Market for the latest version |
| `fs.read.data` | Read its config |
| `fs.write.data` | Write its default config, the settings snippet and downloaded updates |

## Good to know

- Content newer than a player's version appears as the closest older equivalent. For example, copper golems show as frogs on 1.21.8 and older, and happy ghasts show as ghasts on 1.21.5.
- 1.21.5 and older clients can't show dialog screens or locator-bar waypoints, so those are skipped for them.
- 1.20.5 and 1.20.6 players see 1.21's new music discs as older discs (with their real names), new paintings as an older painting of the same size, and don't see enchantments their version doesn't know. Their items keep everything when they go back to the server.
- 1.20.3 and 1.20.4 players see items the classic way (names, lore, enchantments, potions and the rest as item NBT); what their version can't show, such as new attributes, item rarity or food settings, is kept and restored when the item goes back to the server. Armadillos, bogged and the mace appear as cows, strays and a netherite axe, with their real names.
- 1.20.2 and older players see 1.20.3's new blocks and mobs as similar older ones (breezes as blazes, the new tuff and copper blocks as their nearest match); crafter screens don't open for them.
- 1.20 and 1.20.1 have no configuration step, so the plugin goes through it for them. Server resource packs are offered right after they join.
- 1.19.3 and older players see 1.19.4's display entities as invisible markers, sniffers as ravagers and camels as donkeys, each with its real name.
- 1.19, 1.19.1 and 1.19.2 sign chat in a way newer versions cannot check. Their messages are sent unsigned, other players' chat reaches them as plain server messages, and they cannot chat on a server that enforces secure chat.
- Built for Pumpkin 0.2.2 (Minecraft 26.3, plugin API v0.1).

Open source under GPL-3.0. Please report any disconnect message on GitHub so it can be fixed.
