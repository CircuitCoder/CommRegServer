#ifndef __JIEBA_H_
#define __JIEBA_H_

#include <stdlib.h>
#include <stdbool.h>

typedef const void* Jieba;

typedef struct {
  size_t offset;
  size_t len;
} WordView;

typedef struct {
  WordView *list;
  size_t count;
} CutResult;

Jieba create(
  const char* dict_path,
  const char* hmm_path,
  const char* user_dict,
  const char* idf_path,
  const char* stop_words_path);

CutResult cut_for_search(Jieba handle, const char *str, const size_t len);

void free_words(WordView *words);

#endif // __JIEBA_H_
