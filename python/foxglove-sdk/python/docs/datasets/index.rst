Datasets and episodes
=====================

A dataset is a named collection of episodes in a Project. An episode is a time
range over one or more recordings, with optional custom properties such as
``task`` or ``outcome``. Episodes exist independently of datasets, so the same
episode can belong to more than one training or evaluation set.

Datasets do not copy recording bytes. They select episodes (and, after a
commit, pin each episode to a specific episode version). You can inspect that
selection in the Foxglove UI, stream it from Python, or download MCAP files.

Create a dataset, add episodes, trim and label them, then commit a version.
Later episode edits do not change a committed pin. Use the committed version
in training, or clone it into a new dataset.

.. toctree::
   :maxdepth: 1

   explore
   labeling
   pytorch
   ray
   versioning
   pricing
   python-client
   rest
   sdk

What you can do
---------------

On the **Datasets** page, create a dataset or clone one after the first
commit. Add episodes from **Search**, **Recordings**, **Sessions**, **Events**,
or **Episodes**, including their detail pages. In visualization, use **Mark
range** (``Cmd+E``) with **Save as** set to **Episode**, or **Add to dataset**
from the data source menu.

Admins define episode property keys in **Settings → Episodes**. Types are
Single-line text, Multi-line text, Number, True/False, Single-select, and
Multi-select. Filter on the **Episodes** page with **Filter → Properties**
using ``key:value`` terms, or in Search with
``@episode.properties.<key>``. The API ``propertiesQuery`` parameter uses the
same ``key:value`` syntax.

Every dataset has one draft. Adding or removing episodes, or changing episode
version pins, stages changes there. **Review changes** and commit. The
committed version records the episodes, their versions, who committed it, and
when.

On a dataset page, **Use for training** generates a Python script for a
committed version. The CLI and browser download one MCAP per episode plus
``manifest.json``. The ``dataset.version.committed`` webhook fires on commit.

Removing an episode from a dataset does not delete the episode. Deleting a
dataset does not delete its episodes. You can delete an episode only when no
dataset holds it in any version. Deleting an episode does not refund its
creation charge; see :doc:`pricing`.

References
----------

Product UI: https://docs.foxglove.dev/docs/data/datasets

REST: :doc:`rest` and https://docs.foxglove.dev/api/datasets

Python client (``foxglove-client``): :doc:`python-client`

Training adapters (``foxglove-sdk``): :doc:`sdk`

CLI: ``foxglove datasets`` and ``foxglove episodes``
(https://docs.foxglove.dev/docs/cli)

.. note::

   Blog posts on collecting data and on training with Datasets will link from
   here once they are published.
