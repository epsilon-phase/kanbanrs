# v0.2.6

## UI Improvements

* Node text wrap length is now configurable on the preferences screen.

## Refactoring

* Updated packages
* Fixed missing parameters introduced in newer version of layout-rs
* Make the preferences object a singleton

# v0.2.5

## Features
* Autosave
* Preferences
  * Autosave interval configuration
  * Undo storage
  * Default layout on startup

## UI Improvements

* Node layout
  * Added the ability to zoom out on the node layout
    * Current zoom factors of greater than 100% are not enabled s text rendering
      does not work well in egui at higher factors
  * The "updating layout" modal no longer triggers unless it is waiting for
    most of a second
* Added a "Scroll to" button in the editor to navigate to the tasks in question.
* Label the task name entry box


## Bugs

* Creation events in the undo queue are now only merged when they are empty.
* `KanbanDocument::on_tree` will only call the provided function on each task once

## Refactoring

* Use the log crate instead of `println!` calls
* Update egui and associated crates to 0.31.1
* Added a test for `kanban::time_tracking::collect_child_durations`
* Redo some of the layout stuff
  * Renamed `KanbanDocumentLayout` to `KanbanDocumentLayoutType` and replaced it with a
  new struct that also contains a `scroll_to` member

Maintenence:
* Document *much* more of the code.
* Write a getting started section in the readme.

# v0.2.4

## Features
* Window menu - Makes managing windows substantially easier when you've opened too many.
  * Focusing does not work on wayland compositors, but it will attempt to get your
    attention otherwise
  * Editors are listed with a close button.
* Add sorting by the time spent on the task
* Add a menu entry to open all tasks currently recording a time entry
* Files are now saved on a separate thread. You are unlikely to notice a difference.
  * Errors that occur while saving will now be presented as a message.
* Add Filters
  * **Name**  - Match on the name alone.
  * **Tag**   - Match a specific tag
  * **Fuzzy** - Use fuzzy matching
  * **Exact** - Full text exact case-insensitive match

## UI Improvements
* Add a separator between the layout controls and the filter settings.
* Improve the editor's design a bit
  * Add outlines around each column
  * Add a separator between the column function radio selectors and the contents
  * Creating a new category now consists of clicking the edit button,
    which opens a text box and a button to accept the new category
  * Add a group around the button cluster at the bottom
* Add a frame enclosing the filter controls. Makes it easier to follow when the textbox is wrapped
* Very long task names are now wrapped in the node views
* Starting a time entry on a task, when there are other open recordings, will ask what the user
  would like to do
  * Cancel the new time recording
  * Mark the open time recordings as closed
  * Start recording anyway

## Bugs
* Time entry description entries are less congested during editing
* I swear that this time the scrolling is *smooth* and *not jank*

## Refactoring
* Moved the editor display functions into the State impl.
* Rename record_position to record_height to make it clearer what it does.


# v0.2.3

## Features
* Add a view of parent tasks to the editor. This should make getting an idea of the structure easier
  without switching layouts.
* Add an undo stack
* Expand sorting types
  * Sort by newest tasks first.
* Sort tasks in the node layout.

  There may be unusual semantics exposed here
* Time Tracking
  * Per task!
  * Can be added as an instanteous duration or started/finished.
  * Tasks show how time is broken up by their dependencies.
  * Time entry descriptions can be edited after the fact.

    The recorded times themselves represent a more difficult issue so that will come later.
## UI Improvements
* Editors will now be rendered with the `show_viewport_deferred` which allows the program to avoid
  repainting the entire interface whenever an editor needs repainting.
* The grace period before adding a child to a task in the node layout is indicated by the growth
  of the rectangle drawn around the task in question.
* Layouts should have more predictable node orderings(not to say sensible). This may
  have some performance impact if you are a very heavy user.
* Tree Outline layout
  * The Tree outline layout now sorts items more holistically.
  * The Tree outline layout now attempts to account correctly for average item size.
* In-layout Filtering
  * Columnar layout
  * TreeOutline Layout
  * Node Layout
* Filter Features
  * Completion Status
  * Full-text presence
  * Category match
* Improve the editor
  * Change Editor layout
    * Move the add child/ select child to add button cluster to the same column as
      the child task listing.
      * Hide them when it is set to view the parents of the task
  * Mark task name in the editor's window title
* Lighten the shade of red used for the status text
* Dragging a node to the left side of a node will add the dragged node as a parent,
  dragging it over the right side will add it as a child.
* Summaries are now collapsible

## Bugs
* Now saving creates a temporary file and then renames it to the correct file once it's been completely
  written
* Scrolling is now more consistent

## Refactoring
* Use `show_viewport_deferred`
  * Hold the KanbanDocument with an `Arc<RwLock>`
  * Hold the editor state with an `Arc<RwLock>`
* Implement clone for KanbanDocument
* Communicate with task editors using mpsc
* Use parking_lot for rwlocks since apparently it's a lot faster.
* `child_tasks` in the `KanbanItem` is now ordered internally by the task ids rather than
  being arbitrarily ordered
* The `tasks` field in `KanbanDocument` now uses a BTreeMap for consistent ordering.
* The `SummaryAction` enum now includes a command to explicitly indicate that a relayout
  is necessary
* Renamed the EditorRequest enum items

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
