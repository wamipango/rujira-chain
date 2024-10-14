#!/bin/bash

# Set the replacement string from the first argument
replacement=$1

cp -r contracts/rujira-template contracts/rujira-$replacement
cp packages/rujira-rs/src/template.rs packages/rujira-rs/src/$replacement.rs
cd contracts/rujira-$replacement
grep -rl 'rujira-template' . | xargs sed -i "s/rujira-template/rujira-$replacement/g"
grep -rl 'rujira_rs::template' . | xargs sed -i "s/rujira_rs::template/rujira_rs::$replacement/g"
grep -rl 'template::' ./src/bin/schema.rs | xargs sed -i "s/template::/$replacement::/g"
sed -i "1i pub mod $replacement;" ../../packages/rujira-rs/src/lib.rs
cargo run schema
cd -
cargo fmt
