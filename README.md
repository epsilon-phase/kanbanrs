Kanbanrs is a desktop program for doing kanban-style task management written in egui.

# Getting started

This is the way I tend to envision effective task management, but that is likely informed by preference towards
organizing tasks hierarchically, but that isn't necessary for effective task management.

1. Open Kanbanrs
2. Enter the name of the first task you want to add in the textbox labelled "New task name"
3. Click "Add Task"
4. Enter whatever details you want in the editor that opens
5. Click add new child, edit attributes as you please
6. Repeat steps 4-6 as necessary for your task
7. Optionally click Start recording time when you're working on a given task after selecting
   "time tracking" on the task editor
8. Click stop recording on that task when you're done working on it.
9. Mark that task completed when it's done.
10. Save the file, under the "File" menu click "Save" or "Save as"

Further things for you to try:
* Customize category styling etc as you please. It is in the "Edit" menu under *Category Style Editor*
* Edit priority levels. It is in the "Edit" menu under *Priority Editor*
* Try out the node layout (Click the layout combobox and select node layout)
* Search tasks with either the "Search" layout or the "Fuzzy Match" filter selection next to the layout selection.
* Sort tasks according to your needs.

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
* Time tracking

  Keep track of how much time you have spent on a task

# It might do

* Some semblance of being accessible.

  I haven't devoted as much time as is necessary to checking this, Egui is supposedly screenreader
  compatible but I haven't tried it.

# Upcoming features

* Some sort of preferences
* Slightly more accessible node layout(fingers crossed)

# See Also

Read the [Disclaimer](Disclaimer.md) for additional snags you might run into.
