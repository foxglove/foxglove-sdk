# Configuration file for the Sphinx documentation builder.
#
# For the full list of built-in configuration values, see the documentation:
# https://www.sphinx-doc.org/en/master/usage/configuration.html

# -- Project information -----------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#project-information

import os
import sys

from exhale import parse as exhale_parse

sys.path.append(os.path.abspath("."))
from version import SDK_VERSION  # noqa: E402

project = "Foxglove SDK"
copyright = "2025, Foxglove"
author = "Foxglove"
release = SDK_VERSION

# -- General configuration ---------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#general-configuration

extensions = ["breathe", "exhale", "sphinxcontrib.jquery"]

exclude_patterns = ["expected.hpp", "schemas.hpp", ".venv"]

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

# Patch exhale's file-level documentation parser to handle Doxygen XML tags
# that the upstream parser does not understand. Exhale's ``walk`` function
# only recognizes a small subset of Doxygen tags (``para``, ``itemizedlist``,
# ``ref``, ``computeroutput``, etc.) — anything else falls through and its
# text content is concatenated without any reST formatting. In particular:
#
# * ``<programlisting>`` (emitted for ``@code`` blocks) is not turned into a
#   ``.. code-block::`` directive, so file-level code samples render as a
#   single run-on paragraph.
# * ``<sp/>`` tags that Doxygen uses inside ``<programlisting>`` to encode
#   inter-token whitespace are dropped, producing output like
#   ``namespacerdl=foxglove::remote_data_loader_backend``.
# * ``<sect1>``/``<sect2>``/``<title>`` (emitted for ``##`` Markdown headings)
#   lose their underline, so the heading text appears inline with the
#   following paragraph.
#
# Breathe handles all of these correctly when it renders class/struct
# documentation, but exhale uses its own mini-parser for file- and
# namespace-level pages. Wrap that parser to handle the missing tags so file
# documentation renders the same way as struct documentation.
_exhale_walk_orig = exhale_parse.walk

_PROGRAMLISTING_LANG_BY_EXT = {
    ".c": "c",
    ".cpp": "cpp",
    ".css": "css",
    ".html": "html",
    ".js": "javascript",
    ".json": "json",
    ".py": "python",
    ".rb": "ruby",
    ".rs": "rust",
    ".sh": "bash",
    ".text": "text",
    ".ts": "typescript",
    ".unparsed": "text",
    ".yaml": "yaml",
    ".yml": "yaml",
}


def _render_programlisting(tag, indent):
    lang = _PROGRAMLISTING_LANG_BY_EXT.get(tag.attrs.get("filename", ""), "cpp")
    lines = []
    for codeline in tag.find_all("codeline", recursive=False):
        for sp in codeline.find_all("sp"):
            sp.replace_with(" ")
        for ref in codeline.find_all("ref"):
            ref.replace_with(ref.get_text())
        lines.append(codeline.get_text())
    body = "\n".join(indent + "   " + line for line in lines)
    return "\n\n{0}.. code-block:: {1}\n\n{2}\n\n".format(indent, lang, body)


_HEADING_CHAR_BY_SECT = {"sect1": "=", "sect2": "-", "sect3": "~", "sect4": "^"}


def _exhale_walk(textRoot, currentTag, level, prefix=None, postfix=None, unwrapUntilPara=False):
    if currentTag is None:
        return
    name = getattr(currentTag, "name", None)
    indent = "   " * level
    if name == "sp":
        currentTag.replace_with(" ")
        return
    if name == "programlisting":
        currentTag.replace_with(_render_programlisting(currentTag, indent))
        return
    if name in _HEADING_CHAR_BY_SECT:
        title = currentTag.find("title", recursive=False)
        if title is not None:
            text = title.get_text().strip()
            char = _HEADING_CHAR_BY_SECT[name]
            title.replace_with("\n\n{0}\n{1}\n".format(text, char * max(len(text), 3)))
    return _exhale_walk_orig(
        textRoot, currentTag, level, prefix=prefix, postfix=postfix, unwrapUntilPara=unwrapUntilPara
    )


exhale_parse.walk = _exhale_walk
