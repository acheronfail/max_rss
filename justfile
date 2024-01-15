_default:
  just -l

# run the crate, spawning the specified example program
run example *flags:
  cargo build --example {{example}}
  cargo run {{flags}} -- ./target/debug/examples/{{example}}

test:
  cargo test

fmt:
  rustup run nightly cargo fmt
