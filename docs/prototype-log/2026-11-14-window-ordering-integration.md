# 2026-11-14 Initial window ordering integration

This run checks the first integration of the private ordering registry, ordinary-window visibility observations, accepted-input observations, sibling placement and live input refresh with the existing native-window lifecycle. It uses the existing `qa-windows` course; it is not the full ordering acceptance course.

The core was built with `dev-test-games,dev-test-switches` after the initial lifecycle and host-binding integration. Subsequent additions for preserving an original host's logical identity across reconstruction were not in this copied core. The isolated claim `ordering-integrated-1de2b155` was created at 15:52 on 2026-11-14 and started process 78708 hidden. Startup accepted the new dependency points and observers. France was loaded through `menu.start`, paused through `time.pause`, and all seven `qa-windows` markers were answered with `popup.list` and `popup.answer`. No screen-coordinate input was sent.

The native-window course exercised initial construction, row replacement/removal, visibility changes, closing/reopening and mutually exclusive panel replacement. All seven capability calls succeeded. The retained core log contained no ordering panic, window construction failure, or binding failure.

The temporary diagnostic also recorded 54 stable repeated live input refreshes, zero failures and zero missing-ownership deferrals, on main thread 96912 with the presentation lock held. It recorded 42 exact independent traversal matches, zero failures, on the main or drawing thread; drawing thread 54360 handled the last sample.

Release ended the game with exit code 1 and reported no crash. Logs were retained under `ordering-integrated-1de2b155-20261114-155235/kept` in the development claims directory.

Not established by this run: a nontrivial reorder requiring native node movement, shared shortcut precedence, accepted pointer activation, same-batch event freshness, the army attach picker, attached multi-node controls, or native drawing mode. The existing course and diagnostic counters cannot establish those behaviors merely because the run completes.
