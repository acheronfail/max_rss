_default:
  just -l

# run the crate, spawning the specified example program
run example *flags:
  cargo build --example {{example}}
  cargo run {{flags}} -- -d -- ./target/debug/examples/{{example}}
  jq . ./max_rss.json
  jq .max_rss ./max_rss.json | numfmt --to iec

test:
  cargo test

fmt:
  rustup run nightly cargo fmt
