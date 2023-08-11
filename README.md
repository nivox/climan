## CLIMAN

a cool cli http thingy

> this is experimental software, if you want something proven, try https://hurl.dev/

### Usage

```shell
climan [OPTIONS] <SPEC>

Arguments:
  <SPEC>  the path to a YAML specification or 'schema' to print the JSON Schema of climan

Options:
  -v, --variables <VARIABLES>  additional variables to set in the format: FOO=BAR, note that you can also set variables with .env file, the current environmant variables are already available without this option
  -l, --log <LOG>              set this to log the output into the .climan.log file in the current folder [possible values: true, false]
  -h, --help                   Print help
  -V, --version                Print version
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