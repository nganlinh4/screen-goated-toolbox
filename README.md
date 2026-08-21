# Availability feed data

Published by the `Monitor NVIDIA availability` workflow every two hours. This
branch holds only data, never code.

It is deliberately separate from `main`. A job that commits on a schedule and a
human pushing to the same branch collide constantly: in one day the feed produced
eleven of twenty-nine commits on `main` and forced a merge on every push. Keeping
the data here leaves `main` readable and removes the collision entirely.

Nothing here is read directly. Clients fetch `nvidia-availability.json` and verify
it against the public key compiled into the build; an unverified feed is ignored.
`nvidia-history.json` is the publisher's rolling state and is not consumed by
clients.
