# gleipnir [![Build Status](https://travis-ci.com/iovxw/gleipnir.svg?branch=master)](https://travis-ci.com/iovxw/gleipnir)

![screenshot](screenshot.png)

Per-process Network Firewall/Rate Limiter/Monitor for Linux Desktop

## Install

Download the `deb` package from [releases](https://github.com/iovxw/gleipnir/releases/latest), or [build it yourself](#building)

***WARNING**: For compatibility reasons (Qt), the deb contains the client packaged as AppImage. It will be fixed someday*

## Building

*TODO*

## Dependencies

### Build

 - libnetfilter-queue-dev
 - libdbus-1-dev
 - *FIXME*

### Runtime

Qt >= 5.10

 - libnetfilter-queue1
 - libdbus-1-3
 - qml-module-qtquick-dialogs
 - qml-module-qtquick2
 - qml-module-qtgraphicaleffects
 - qml-module-qtcharts
 - *FIXME*

## Repository structure

Three parts:

### Daemon

`gleipnird` is the daemon which does the real work, it only requires a little memory (about 4MiB, dependent on the number of rules), and run as superuser

### Client

`gleipnir` written in QML, allows users to view/edit rules and monitor network traffic

### Library

`gleipnir-interface`, just some shared structs and RPC interfaces

## TODO
 - [ ] Performance (currently, everything is just work)
 - [ ] eBPF backend
 - [ ] Better UI/UX

## License

This is free and unencumbered software released into the public domain.

Anyone is free to copy, modify, publish, use, compile, sell, or distribute this software, either in source code form or as a compiled binary, for any purpose, commercial or non-commercial, and by any means.
