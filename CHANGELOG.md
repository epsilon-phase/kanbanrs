# v0.2.3

## Features

## UI Improvements
* Editors will now be rendered with the `show_viewport_deferred` which allows the program to avoid
  repainting the entire interface whenever an editor needs repainting.
* The grace period before adding a child to a task in the node layout is indicated by the growth
  of the rectangle drawn around the task in question.

## Bugs
* Now saving creates a temporary file and then renames it to the correct file once it's been completely
  written

## Refactoring
* Use `show_viewport_deferred`
  * Hold the KanbanDocument with an `Arc<RwLock>`
  * Hold the editor state with an `Arc<RwLock>`
* Implement clone for KanbanDocument
* Communicate with task editors using mpsc

# v0.2.2

## Features
* Accepts a filename to open from the command line arguments.
* Accept a layout to open to from the command line arguments.

## UI Improvements
* The node layout now uses bezier curves, as layout-rs emits.
* Add confirmation dialog when quitting without having saved.
* Dragging and dropping now has a grace period where it will not add
  the dropped-onto element into the children of the dragged element

# v0.2.1

## Features

* Indicate if the node may be added as a child when dragging in the node layout
* Categories can be set to be inherited by newly created children.
* Nodes may be collapsed in the node layout to hide all their descendants

## Improve UI:
* Mark the add child button in the task editor as disabled when there is
  no child selected, rather than just not being drawn.
Bugs:
* Id assignment should now work even if the id type wraps around.

# v.0.2.0

## Features

* The node layout displays tasks with *most* of the category styling.
* New documents now start with "High", "Medium", and Low priorities defined
* Show parents and children when a node is focused in the node layout
* Sort the child task's comboboxes by creation order/completion status
* Mark the child task combobox's task completion status
* The xdg dependency is no longer specified for windows.
* Label some comboboxes better(On the right side)

# v0.1.0

## Features
* Mark completed tasks as green in the node layout(undone)
* Add node layout
* Tree outline view
* Priority editor
* Category editor
* Fuzzy text matching for search
