#include "namespace.h"

#include <errno.h>
#include <jni.h>
#include <stdlib.h>
#include <string.h>

static char *name_bytes(JNIEnv *env, jbyteArray bytes) {
    if (bytes == NULL) return NULL;
    jsize size = (*env)->GetArrayLength(env, bytes);
    if (size <= 0 || size > 4096) return NULL;
    char *name = malloc((size_t)size + 1);
    if (name == NULL) return NULL;
    (*env)->GetByteArrayRegion(env, bytes, 0, size, (jbyte *)name);
    if ((*env)->ExceptionCheck(env) || memchr(name, '\0', (size_t)size) != NULL) {
        free(name);
        return NULL;
    }
    name[size] = '\0';
    return name;
}

JNIEXPORT jint JNICALL
Java_dev_screengoated_toolbox_mobile_phonecontrol_provider_fileio_FileNamespaceNative_openDirectory(
    JNIEnv *env, jobject receiver, jbyteArray bytes) {
    (void)receiver;
    char *path = name_bytes(env, bytes);
    if (path == NULL) return -EINVAL;
    int result = sgt_open_directory(path);
    free(path);
    return result;
}

JNIEXPORT jint JNICALL
Java_dev_screengoated_toolbox_mobile_phonecontrol_provider_fileio_FileNamespaceNative_openEntry(
    JNIEnv *env, jobject receiver, jint directory, jbyteArray bytes, jint kind) {
    (void)receiver;
    char *name = name_bytes(env, bytes);
    if (name == NULL) return -EINVAL;
    int result = sgt_open_entry(directory, name, kind);
    free(name);
    return result;
}

JNIEXPORT jint JNICALL
Java_dev_screengoated_toolbox_mobile_phonecontrol_provider_fileio_FileNamespaceNative_lockFile(
    JNIEnv *env, jobject receiver, jint descriptor, jboolean acquire) {
    (void)env;
    (void)receiver;
    return sgt_lock_file(descriptor, acquire ? 1 : 0);
}

JNIEXPORT jint JNICALL
Java_dev_screengoated_toolbox_mobile_phonecontrol_provider_fileio_FileNamespaceNative_syncDirectory(
    JNIEnv *env, jobject receiver, jint directory) {
    (void)env;
    (void)receiver;
    return sgt_sync_directory(directory);
}

JNIEXPORT jint JNICALL
Java_dev_screengoated_toolbox_mobile_phonecontrol_provider_fileio_FileNamespaceNative_makeDirectory(
    JNIEnv *env, jobject receiver, jint directory, jbyteArray bytes) {
    (void)receiver;
    char *name = name_bytes(env, bytes);
    if (name == NULL) return EINVAL;
    int result = sgt_mkdir_entry(directory, name);
    free(name);
    return result;
}

JNIEXPORT jint JNICALL
Java_dev_screengoated_toolbox_mobile_phonecontrol_provider_fileio_FileNamespaceNative_publish(
    JNIEnv *env, jobject receiver, jint directory, jbyteArray old_bytes,
    jbyteArray new_bytes, jboolean exchange) {
    (void)receiver;
    char *source = name_bytes(env, old_bytes);
    char *target = name_bytes(env, new_bytes);
    int result = source != NULL && target != NULL ?
        sgt_publish_entry(directory, source, target, exchange ? 1 : 0) : EINVAL;
    free(source);
    free(target);
    return result;
}

JNIEXPORT jint JNICALL
Java_dev_screengoated_toolbox_mobile_phonecontrol_provider_fileio_FileNamespaceNative_moveNoReplace(
    JNIEnv *env, jobject receiver, jint source_directory, jbyteArray old_bytes,
    jint target_directory, jbyteArray new_bytes) {
    (void)receiver;
    char *source = name_bytes(env, old_bytes);
    char *target = name_bytes(env, new_bytes);
    int result = source != NULL && target != NULL ?
        sgt_move_entry(source_directory, source, target_directory, target) : EINVAL;
    free(source);
    free(target);
    return result;
}

JNIEXPORT jint JNICALL
Java_dev_screengoated_toolbox_mobile_phonecontrol_provider_fileio_FileNamespaceNative_unlinkStagedFile(
    JNIEnv *env, jobject receiver, jint directory, jbyteArray bytes, jint expected_fd) {
    (void)receiver;
    char *name = name_bytes(env, bytes);
    if (name == NULL) return EINVAL;
    int result = sgt_unlink_staged_file(directory, name, expected_fd);
    free(name);
    return result;
}
