#include "cppjieba/Jieba.hpp"
#include <vector>

extern "C" {
  #include "jieba.h"

  using namespace std;

  Jieba create(
    const char* dict_path,
    const char* hmm_path,
    const char* user_dict,
    const char* idf_path,
    const char* stop_words_path) {
      return (Jieba)(new cppjieba::Jieba(dict_path, hmm_path, user_dict, idf_path, stop_words_path));
    }

  CutResult cut_for_search(Jieba handle, const char *str, const size_t len) {
    auto jieba = (cppjieba::Jieba *) handle;
    vector<cppjieba::Word> words;
    string s(str, len);
    jieba->CutForSearch(s, words);

    auto results = new WordView[words.size()];

    for(size_t i = 0; i < words.size(); ++i) {
      results[i].offset = words[i].offset;
      results[i].len = words[i].word.size();
    }

    return CutResult{ results, words.size() };
  }

  void free_words(WordView *words) {
    delete[] words;
  }
}
