Notebook integration
====================

Functions and classes for integrating with Jupyter notebooks and creating interactive visualizations.

.. note::
   The notebook integration classes and functions are only available when the ``notebook`` extra package is installed.
   Install it with ``pip install foxglove-sdk[notebook]``.

.. autofunction:: foxglove.init_notebook_buffer

.. autoclass:: foxglove.notebook.notebook_buffer.NotebookBuffer
   :members:
   :exclude-members: __init__

.. autoclass:: foxglove.notebook.foxglove_widget.FoxgloveWidget
   :members:
   :exclude-members: __init__

Layouts
^^^^^^^

.. automodule:: foxglove.layouts
   :members: StackContainer, StackItem, SplitContainer, SplitItem, TabContainer, TabItem, Layout, UserScript, Panel, BasePanel
   :undoc-members:
   :exclude-members: __init__

Panels
^^^^^^

.. foxglove_autopanels::
