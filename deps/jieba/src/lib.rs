#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::os::raw::c_char;
use std::ffi::{CString};
// TODO: windows compatiiblity
use std::os::unix::ffi::OsStrExt;

mod jieba_int {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

#[derive(Debug)]
pub enum JiebaError {
    NulError,
}

fn pathbuf_to_cstring(p: PathBuf) -> Result<CString, JiebaError> {
    CString::new(p.as_os_str().as_bytes()).map_err(|_| JiebaError::NulError)
}

pub struct JiebaWords<'a> {
    template: &'a str,
    list: *mut jieba_int::WordView,
    count: usize,
}

impl<'a> Drop for JiebaWords<'a> {
    fn drop(&mut self) {
        unsafe {
            jieba_int::free_words(self.list);
        }
    }
}

pub struct JiebaWordsIter<'a> {
    words: &'a JiebaWords<'a>,
    count: usize,
    ptr: *mut jieba_int::WordView,
}

impl<'a> Iterator for JiebaWordsIter<'a> {
    type Item = &'a str;
    fn next(&mut self) -> Option<Self::Item> {
        if self.count == self.words.count {
            return None;
        }

        Some(unsafe {
            let from = (*self.ptr.add(self.count)).offset;
            let till = (*self.ptr.add(self.count)).len + from;
            self.count += 1;

            &self.words.template[from..till]
        })
    }
}

impl<'a> JiebaWords<'a> {
    pub fn iter(&'a self) -> JiebaWordsIter<'a> {
        JiebaWordsIter {
            words: self,
            ptr: self.list,
            count: 0,
        }
    }
}

pub struct Jieba {
    handle: jieba_int::Jieba,
}

unsafe impl Sync for Jieba {}

impl Jieba {
    pub fn new<P: AsRef<Path>>(dictdir: P) -> Result<Jieba, JiebaError> {
        let p: &Path = dictdir.as_ref();
        let handle = unsafe {
            jieba_int::create(
                pathbuf_to_cstring(p.join("jieba.dict.utf8"))?.as_ptr(),
                pathbuf_to_cstring(p.join("hmm_model.utf8"))?.as_ptr(),
                pathbuf_to_cstring(p.join("user.dict.utf8"))?.as_ptr(),
                pathbuf_to_cstring(p.join("idf.utf8"))?.as_ptr(),
                pathbuf_to_cstring(p.join("stop_words.utf8"))?.as_ptr(),
            )
        };
        Ok(Jieba{ handle })
    }

    pub fn cut_for_search<'a>(&self, tmpl: &'a str) -> Result<JiebaWords<'a>, JiebaError> {
        let ctmpl = match CString::new(tmpl) {
            Err(_) => return Err(JiebaError::NulError),
            Ok(s) => s,
        };

        let result = unsafe {
            let result = jieba_int::cut_for_search(self.handle, ctmpl.as_ptr(), tmpl.len());

            JiebaWords {
                template: tmpl,
                list: result.list,
                count: result.count,
            }
        };

        Ok(result)
    }
}
