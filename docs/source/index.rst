Voxel game engine
=================

Rust + Vulkan workspace with custom ECS, chunked voxels, meshing, and a unified editor binary that supports both embedded and external engine-runner workflows.

.. toctree::
   :maxdepth: 2
   :caption: User guide

   editor
   projects
   prefabs
   scripting

Other references
----------------

* Repository ``README.md`` — prerequisites, ``cargo`` commands, project workflow, crate index.
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
