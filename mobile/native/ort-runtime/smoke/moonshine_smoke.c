#include "moonshine-c-api.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static uint16_t read_u16(FILE* file) {
  uint8_t bytes[2];
  if (fread(bytes, 1, sizeof(bytes), file) != sizeof(bytes)) return 0;
  return (uint16_t)(bytes[0] | ((uint16_t)bytes[1] << 8));
}

static uint32_t read_u32(FILE* file) {
  uint8_t bytes[4];
  if (fread(bytes, 1, sizeof(bytes), file) != sizeof(bytes)) return 0;
  return (uint32_t)bytes[0] | ((uint32_t)bytes[1] << 8) |
         ((uint32_t)bytes[2] << 16) | ((uint32_t)bytes[3] << 24);
}

static int seek_chunk(FILE* file, const char expected[4], uint32_t* size) {
  char id[4];
  while (fread(id, 1, sizeof(id), file) == sizeof(id)) {
    const uint32_t chunk_size = read_u32(file);
    if (memcmp(id, expected, sizeof(id)) == 0) {
      *size = chunk_size;
      return 1;
    }
    if (fseek(file, (long)(chunk_size + (chunk_size & 1)), SEEK_CUR) != 0) return 0;
  }
  return 0;
}

static float* load_pcm16_wav(const char* path, uint64_t* count, int32_t* rate) {
  FILE* file = fopen(path, "rb");
  if (file == NULL) return NULL;
  char header[4];
  uint32_t chunk_size;
  float* samples = NULL;
  if (fread(header, 1, 4, file) != 4 || memcmp(header, "RIFF", 4) != 0 ||
      fseek(file, 4, SEEK_CUR) != 0 || fread(header, 1, 4, file) != 4 ||
      memcmp(header, "WAVE", 4) != 0 || !seek_chunk(file, "fmt ", &chunk_size) ||
      chunk_size < 16) {
    goto done;
  }
  const uint16_t format = read_u16(file);
  const uint16_t channels = read_u16(file);
  const uint32_t sample_rate = read_u32(file);
  (void)read_u32(file);
  (void)read_u16(file);
  const uint16_t bits = read_u16(file);
  if (chunk_size > 16 && fseek(file, (long)(chunk_size - 16), SEEK_CUR) != 0) goto done;
  if (format != 1 || channels != 1 || bits != 16 || sample_rate == 0 ||
      !seek_chunk(file, "data", &chunk_size)) {
    goto done;
  }
  const uint64_t sample_count = chunk_size / sizeof(int16_t);
  samples = (float*)malloc((size_t)sample_count * sizeof(float));
  if (samples == NULL) goto done;
  for (uint64_t i = 0; i < sample_count; ++i) {
    const int16_t sample = (int16_t)read_u16(file);
    samples[i] = (float)sample / 32768.0f;
  }
  *count = sample_count;
  *rate = (int32_t)sample_rate;
done:
  fclose(file);
  return samples;
}

int main(int argc, char** argv) {
  if (argc != 3) {
    fprintf(stderr, "usage: %s MODEL_DIR PCM16_MONO_WAV\n", argv[0]);
    return 2;
  }
  uint64_t sample_count = 0;
  int32_t sample_rate = 0;
  float* samples = load_pcm16_wav(argv[2], &sample_count, &sample_rate);
  if (samples == NULL) {
    fprintf(stderr, "failed to load WAV\n");
    return 3;
  }
  const int32_t transcriber = moonshine_load_transcriber_from_files(
      argv[1], MOONSHINE_MODEL_ARCH_TINY_STREAMING, NULL, 0,
      MOONSHINE_HEADER_VERSION);
  if (transcriber < 0) {
    fprintf(stderr, "load failed: %s\n", moonshine_error_to_string(transcriber));
    free(samples);
    return 4;
  }
  struct transcript_t* transcript = NULL;
  const int32_t result = moonshine_transcribe_without_streaming(
      transcriber, samples, sample_count, sample_rate, 0, &transcript);
  int matched = 0;
  if (result == 0 && transcript != NULL) {
    for (uint64_t i = 0; i < transcript->line_count; ++i) {
      const char* text = transcript->lines[i].text;
      printf("%s\n", text == NULL ? "" : text);
      if (text != NULL && strstr(text, "best of times") != NULL) matched = 1;
    }
  }
  const int succeeded = result == 0 && transcript != NULL && matched;
  moonshine_free_transcriber(transcriber);
  free(samples);
  if (!succeeded) {
    fprintf(stderr, "representative transcription failed: %s\n",
            moonshine_error_to_string(result));
    return 5;
  }
  return 0;
}
