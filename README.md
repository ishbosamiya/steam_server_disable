# Steam Server Disable

A GUI tool to block access to Steam (CSGO/Dota) servers region-wise by
making use of the system firewall. This is useful when joining servers
from some region is not optimal.

## Features

* Cross Platform (Linux and Windows)
* GUI
* Real-time ping + loss check

## Note

It requires sudo/administrator access since it modifies firewall rules
and pinging is done through ICMP which requires raw sockets.

Steam adds (and/or removes) servers every so often. So click on
`Download Server List` every few days to keep the list up-to date.

This only blocks direct access to the servers that are "disabled". It
is still possible to connect to a "disabled" server since steam has
internals routes between the servers and leverages this network when
direct access is not possible. The benefit to blocking some region is
that steam would preferentially choose a server with a direct route
thus drastically reducing the chances of connecting to a "disabled"
server.

### Linux

The rules are added via `iptables` and thus do not persist between
shutdowns. So rerun after restarting. This might be updated in the
future by using `ufw` instead.

## Installation
### Prepackaged Binaries
#### Github Releases

Download the appropriate file from
[Releases](https://github.com/ishbosamiya/steam_server_disable/releases).

##### Windows

Completely portable. See [Usage](#usage).

##### Linux (Debian/Ubuntu/Pop!_OS)

``` shell
sudo dpkg -i steam_server_disable_*_.deb
```

##### Linux (Portable)

Extract file then see [Usage](#usage).

## Usage
### Windows

Right click -> Run as Administrator

### Linux (Debian/Ubuntu/Pop!_OS)

``` shell
steam_server_disable
```

### Linux (Portable)

``` shell
./steam_server_disable
```

## Build

``` shell
git clone https://github.com/ishbosamiya/steam_server_disable.git
cd steam_server_disable
cargo run --release
```
The executable generated is portable.

## Screenshot

![Version 0.2.2+](/screenshots/v0_2_2+.png)

## TODO

* [ ] Use `ufw` instead of `iptables` for Linux

## Disclaimer

Use on your own risk. It is unlikely that steam would ban a user
blocking access to some of their servers (ISPs can do this and steam
would never know if it is the user who has blocked access or the ISP)
but there is always a very very small chance.
