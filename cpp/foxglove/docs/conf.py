# Configuration file for the Sphinx documentation builder.
#
# For the full list of built-in configuration values, see the documentation:
# https://www.sphinx-doc.org/en/master/usage/configuration.html

# -- Project information -----------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#project-information

import os
import sys

sys.path.append(os.path.abspath("."))
from version import SDK_VERSION  # noqa: E402

project = "Foxglove SDK"
copyright = "2025, Foxglove"
author = "Foxglove"
release = SDK_VERSION

# -- General configuration ---------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#general-configuration

extensions = ["breathe", "exhale", "sphinxcontrib.jquery"]

exclude_patterns = ["expected.hpp", ".venv"]

primary_domain = "cpp"
highlight_language = "cpp"


# -- Options for HTML output -------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#options-for-html-output

html_theme = "furo"

# Breathe extension: https://breathe.readthedocs.io

breathe_projects = {"Foxglove SDK": "../../build/docs/xml"}
breathe_default_project = "Foxglove SDK"

# Exhale extension: https://exhale.readthedocs.io

exhale_args = {
    "containmentFolder": "./generated/api",
    "contentsDirectives": html_theme != "furo",  # Furo does not support this
    "createTreeView": False,
    "doxygenStripFromPath": "..",
    "exhaleExecutesDoxygen": False,
    "rootFileName": "library_root.rst",
    "rootFileTitle": "API Reference",
}
