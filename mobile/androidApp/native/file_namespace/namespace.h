#ifndef SGT_FILE_NAMESPACE_H
#define SGT_FILE_NAMESPACE_H

/* Open returns an owned fd or negative errno; mutations return zero or errno. */
int sgt_open_directory(const char *path);
int sgt_open_entry(int directory, const char *name, int kind);
int sgt_sync_directory(int directory);
int sgt_lock_file(int descriptor, int acquire);
int sgt_mkdir_entry(int directory, const char *name);
int sgt_publish_entry(int directory, const char *source, const char *target, int exchange);
int sgt_move_entry(int source_directory, const char *source, int target_directory, const char *target);
int sgt_unlink_staged_file(int directory, const char *name, int expected_fd);

#endif
