#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::path::Path;

mod jieba_int {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub struct Jieba {
    handle: jieba_int::Jieba,
}

impl Jieba {
    fn new<P: AsRef<Path>>(dictdir: P) -> Jieba {
        Jieba {
            unsafe {
                NewJieba(
                    dictdir.join("jieba.dict.utf8"),
                    dictdir.join("hmm_model.utf8"),
                    dictdir.join("user.dict.utf8"),
                    dictdir.join("idf.utf8"),
                    dictdir.join("stop_words.utf8"),
                ),
            }
        }
    }

    fn cut_for_search(str: String) ->
}
