
# Features so far

* Fuzzy searching

  You can search tasks using fuzzy matching over multiple fields.
* "Lightweight"

  The document I use to track development only allocates 17 mb to the heap
* Convenient for dogfooding its own development
* Tags

  Sometimes title and description fields are just not enough

* Queue view

  Task trackers that focus on kanban tends to allow for choice paralysis(especially if you're procrastinating
  like I often do), if you use priorities and the task dependency system then this will allow you
  to see the most important unblocked tasks first.

* Focus View

  Focus on related tasks.

* Categories
* Category based styling - Allows styling the presentation of tasks based on what they are.
* Sorting
* Graphviz-like dependency visualization (Node layout)

  View how the tasks relate to each other with a graphical display
  with a lot of similarities to graphviz(This uses "layout-rs" which is
  very similar but not identical)
* Task tree outline

  This is a more basic layout, indenting the tasks to the depth they
  appear in a tasks's tree.

# It might do

* Some semblance of being accessible.

  I haven't devoted as much time as is necessary to checking this, Egui is supposedly screenreader
  compatible but I haven't tried it.

# Upcoming features

* Some sort of preferences
* Slightly more accessible node layout(fingers crossed)
