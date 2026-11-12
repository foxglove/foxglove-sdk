foxglove.client (foxglove-client package)
=========================================

``foxglove.client`` is the Python client for the Foxglove API. It is published separately from the
SDK, as the `foxglove-client <https://pypi.org/project/foxglove-client/>`__ package, and its source
is in the `foxglove-python <https://github.com/foxglove/foxglove-python>`__ repository. The
``torch`` and ``ray`` extras of ``foxglove-sdk`` install it.

This page is generated from the docstrings of ``foxglove-client`` |foxglove_client_version|. It
covers the methods of :py:class:`~foxglove.client.Client` that work with datasets, episodes, and
custom properties. All of them are available in version 0.23 and later.

Pass timezone-aware ``datetime`` values for time arguments. The client converts them with
``astimezone()``, so a naive ``datetime`` is read as local time. Times in results, such as
``created_at`` and ``start_time``, are ``datetime`` objects.

The REST API that these methods call is documented at https://docs.foxglove.dev/api.

.. currentmodule:: foxglove.client

.. autoclass:: Client
   :exclude-members: __new__

Datasets
--------

.. automethod:: Client.create_dataset
.. automethod:: Client.get_datasets
.. automethod:: Client.get_dataset
.. automethod:: Client.update_dataset
.. automethod:: Client.delete_dataset
.. automethod:: Client.get_dataset_episodes
.. automethod:: Client.update_dataset_episodes
.. automethod:: Client.get_dataset_versions
.. automethod:: Client.get_dataset_version
.. automethod:: Client.update_dataset_version
.. automethod:: Client.get_dataset_version_episodes
.. automethod:: Client.compare_dataset_versions
.. automethod:: Client.commit_dataset
.. automethod:: Client.discard_dataset
.. automethod:: Client.restore_dataset_version
.. automethod:: Client.clone_dataset_version
.. automethod:: Client.download_dataset

Episodes
--------

.. automethod:: Client.create_episodes
.. automethod:: Client.get_episodes
.. automethod:: Client.get_episode
.. automethod:: Client.update_episode
.. automethod:: Client.update_episodes
.. automethod:: Client.delete_episode
.. automethod:: Client.get_episode_versions
.. automethod:: Client.iter_messages

Custom properties
-----------------

The ``properties`` of an episode use keys that are defined as custom properties with
``resource_type="episode"``.

.. automethod:: Client.create_custom_property
.. automethod:: Client.get_custom_properties
.. automethod:: Client.get_custom_property
.. automethod:: Client.update_custom_property
.. automethod:: Client.delete_custom_property

Supporting types
----------------

.. autoclass:: Page
   :members:
   :exclude-members: __init__, __new__

.. autoclass:: DatasetDownloadWarning
   :exclude-members: __init__, __new__
