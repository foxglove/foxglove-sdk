# foxglove-notebook

## Installation

```sh
pip install foxglove-notebook
```

## Development

Install front-end dependencies:

```sh
yarn install
```

Build front-end assets

```sh
yarn workspace @foxglove/notebook dev
```

Install python dependencies

```sh
poetry install
```

Activate poetry environment

```sh
eval $(poetry env activate)
```

Install widget in editable mode

```sh
pip install -e .
```

Launch jupyter lab

```sh
jupyter lab
```
