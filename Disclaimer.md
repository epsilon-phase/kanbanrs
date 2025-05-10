# Disclaimer

## Suitability and Stability for serious usage

While I have taken a great deal of care to ensure that this project does not have crashes and freezes,
I do not have a lot of users, or any bug reporters. I must make it clear that the reason there are no bug
reports is not at all related to their total absence.

This program uses a three stage save system to attempt to ensure that crashes in the middle of saving does
not corrupt or destroy the previous version, and that consists of

1. Writing the new version to a file called `<document>.kan.bak`
2. Deleting the old version
3. Renaming the new version to `<document>.kan`

This process is used in a large number of programs to ensure that the state of
a document being edited is always at a 'known good' state, however this does not
account for multiple users attempting to edit the same file, as merging the document
states would be complicated and lock files are not my preferred solution.

The preferred solution is to *not have more than one user editing a kanban document at once*,
however, if there is demand for accounting for this use case, there will be two options available,

1. Store a timestamp of the last modification/save in the loaded structure and compare to the file each time
   it is saved.
2. The lock file.

However lock files can produce spurious warnings in the event of a sudden unaccounted-for shutdown.

### The Undo system

The undo system is fairly simple, it consists of three types of events:
* Creation events
* Deletion events
* Modification events

Each of these events contains the information required to undo the modification to the document.
However, I have not gone through the trouble of verifying, formally or not, that this  is correct
under all circumstances. In addition, this currently uses a circular buffer that limits the total
depth of the undo history.

Creative usage of this program *may* result in inconsistent state after an undo. There is also no redo system

### Suitability for the search system

Fuzzy matching is great for some circumstances, but the way that it filters things is not the most intuitive.

## Name conflicts
This project does not have any relationship to another program called [kanbanrs](https://github.com/SkaneroOo/kanbanrs),
and although that project has not been updated in several years, I believe it may have preceded it. If you are the owner
of that project and wish to take this crate and name from me, please make an issue.
