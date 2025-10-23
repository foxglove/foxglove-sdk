# Configuration file for the Sphinx documentation builder.
#
# For the full list of built-in configuration values, see the documentation:
# https://www.sphinx-doc.org/en/master/usage/configuration.html
from datetime import date

from docs.version import SDK_VERSION

# -- Project information -----------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#project-information

project = "Foxglove SDK"
copyright = f"{date.today().year}, Foxglove"
author = "Foxglove"
release = SDK_VERSION

# -- General configuration ---------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#general-configuration

extensions: list[str] = [
    "sphinx.ext.autodoc",
    "sphinx_autodoc_typehints",
    "enum_tools.autoenum",
]

nitpicky = True
nitpick_ignore_regex = [
    # Ignore warnings for built-in types from autodoc_typehints
    ("py:data", r"typing.*"),
    ("py:class", r"collections\.abc\.Callable"),
    ("py:class", r"Path"),
    # autodoc_typehints also fails on Capability which is imported in websocket.py, but is
    # manually documented as an enum
    ("py:class", r"foxglove\.Capability"),
]

templates_path = ["_templates"]
exclude_patterns = ["_build", "Thumbs.db", ".DS_Store"]

# Config for sphinx_autodoc_typehints
# https://pypi.org/project/sphinx-autodoc-typehints/
typehints_defaults = "braces"


# -- Options for HTML output -------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#options-for-html-output

html_theme = "alabaster"
