# Fancy Zip Packers

Creates a collection of zip files of an approximate size of a given folder, which can be extracted on top of each other in any order.

Each directory within the root folder can be given different compression levels.

Each file not included in a specific configured directory will be instead included in the root zip.

## Config

See `config.toml.example`