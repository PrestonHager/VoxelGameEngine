Voxel game engine
=================

Rust + Vulkan workspace: custom ECS, chunked voxels, meshing, optional **editor** with IPC to **engine-runner**.

.. toctree::
   :maxdepth: 2
   :caption: User guide

   editor
   prefabs
   scripting

Other references
----------------

* Repository ``README.md`` — prerequisites, ``cargo`` commands, crate index.
* ``agents.md`` — architecture decisions and phased roadmap.

Build these docs locally
------------------------

Install dependencies (from the ``docs`` directory):

.. code-block:: bash

   pip install -r requirements.txt

Then:

.. code-block:: bash

   cd docs/source
   sphinx-build -b html . ../_build

Open ``docs/_build/index.html`` in a browser.
