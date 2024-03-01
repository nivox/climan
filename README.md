## CLIMAN

a cool cli http thingy

> this is experimental software, if you want something proven, try https://hurl.dev/

### Usage

```shell
Usage: climan [OPTIONS] <COMMAND>

Commands:
  workflow  Executes a workflow
  request   Executes a single request
  schema    Prints the schema for the workflow
  help      Print this message or the help of the given subcommand(s)

Options:
  -l, --log <LOG>  set this to log the output into the .climan.log file in the current folder [possible values: true, false]
  -h, --help       Print help
  -V, --version    Print version
```

#### Editing

To get schema completion in VS Code you can use the schema from this repo or write the schema into a file using climan itself:

> climan schema > climan.schema.json

You can set the schema in the `settings.json` fields of the [YAML extension](https://marketplace.visualstudio.com/items?itemName=redhat.vscode-yaml):
```json
    "yaml.schemas": {
        "<path_to_schema_file>/climan.schema.json": ".climan.yaml"
    },
```

Your climan specs need to have a `.climan.yaml` ending to work with this.

### Development

needs rust 1.71.1 or higher

## License

[MIT](LICENSE)
