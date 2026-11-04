# tokencamp-tools

Tools for using [tokencamp](https://tokencamp.com) from the command line.

## tcamp

`tcamp` is the first tool in the toolkit: a single binary that configures a
harness to use tokencamp and launches it. Your API key stays in a `0600`
credential file and is injected at launch — it is never written into harness
config.

**Status: v0.1 in progress.** There is no install command yet. Watch this
repository for the first release.

v0.1 targets Claude Code, Pi, and Kimi Code.

The design is specified in [SPEC.md](./SPEC.md); terminology lives in
[GLOSSARY.md](./GLOSSARY.md).

## License

Apache-2.0.
