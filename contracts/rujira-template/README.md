# RUJIRA Template

This readme is an example of how to setup your repo correctly to work with THORChain. 

# Implementation

This is how it works

## Whitelisting on Base Layer

THORChain explorers use the merged WASMPERMISSIONS cargo.toml 

Origin: link to this repo

### Example

https://thornode.ninerealms.com/thorchain/codes
```json
{"code":"11ddc91557ec8ea845b74ceb6b9f5502672e8a856b0c1752eb0ce19e3ad81dac",
"deployers":["thor1e0lmk5juawc46jwjwd0xfz587njej7ay5fh6cd"],
"origin":"https://gitlab.com/thorchain/rujira/-/tree/8cc96cf59037a005051aff2fd16e46ff509a9241/contracts/rujira-fin"}
```


### Cargo.toml

THORChain explorers will query the origin URL and retrieve values on the cargo.toml file. In particular:

```yaml
[package]
name = "rujira-template" //contract name
version = "1.0.0" //keep this up to date
authors = [] //handles
audits = ["link/to/your/audit"]
auditors = ["NAME"]
docs = "link/to/your/docs"
```

This will then be displayed here
https://thorchain.net/rujira/contracts

