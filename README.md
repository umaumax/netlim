# netlim

This is a bandwidth limiting tool for TCP network communication.

## how to install
``` bash
cargo install --git https://github.com/umaumax/netlim
```

## how to check
wget
``` bash
# mac
mkfile -v 100m 100m
# linux
fallocate -l 100m 100m

# server
python3 -m http.server 22222

# bandwidth limiter
cargo run -- --src=0.0.0.0:11111 --dst=localhost:22222

# client
wget localhost:11111/100m
```

iperf3
``` bash
# server
iperf3 -s

# client
netlim --src 0.0.0.0:5201 --dst $server_ip_addr:5201 --out 1MB --in 1MB

iperf3 -c localhost -p 5201
```

alternative command
``` bash
ssh -g -N -L 5201:localhost:5201 $server_ip_addr -o ProxyCommand='pv -cN out -L 1024K | nc %h %p | pv -cN in -L 1024K'
```

## how to use
``` bash
# with debug logs
cargo run -- --src=0.0.0.0:11111 --dst=localhost:22222 --verbose
```

## help
``` bash
netlim 0.1.0

USAGE:
    netlim [FLAGS] [OPTIONS]

FLAGS:
    -h, --help       Prints help information
        --unshare    unshare bandwidth limit flag
    -V, --version    Prints version information
        --verbose    verbose flag

OPTIONS:
        --dst <dst-socket>                  dst socket [default: localhost:22222]
        --in <inbound-bandwidth-limit>      inbound bandwidth limit [Byte] [default: 1MB]
        --out <outbound-bandwidth-limit>    outbound bandwidth limit [Byte] [default: 1MB]
        --src <src-socket>                  src socket [default: 0.0.0.0:11111]
```

## figure
``` mermaid
flowchart LR
    client -- "send to src socket (outbound)" --> netlim -- "response (inbound)" --> client
    server -- "response (inbound)" --> netlim -- "send to dst socket (outbound)" --> server
```

## NOTE
* [tikv/async-speed-limit: Asynchronously speed-limiting multiple byte streams]( https://github.com/tikv/async-speed-limit )
  * これは`futures`向けであるが、`futures-util`のcompatモジュールを利用すると利用できるらしいが、うまくいかない
* inboundやoutboundのデータ量に応じて、特定の時間までsleepする仕組みで帯域制限を行っている
* デフォルトではコネクションで帯域制限を共有している
  * ただし、コネクションごとのスケジューリングはtokio依存であるので、平等なスケジューリングとはならないことに注意
  * コネクションごとに独立して帯域制限するオプションあり(`--unshare`)
