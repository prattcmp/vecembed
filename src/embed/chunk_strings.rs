use std::cmp;
use std::pin::Pin;
use std::str;
use std::task::{Context, Poll};
use futures::Stream;

use crate::embed::instances::get_tokenizer_instance;

use super::errors::EmbeddingError;

pub struct StringChunkIterator<'a> {
    chunkable: &'a str,
    max_chunk_size: usize,
    start: usize,
}

impl<'a> StringChunkIterator<'a> {
    pub fn new(chunkable: &'a str, max_chunk_size: usize) -> Self {
        StringChunkIterator {
            chunkable,
            max_chunk_size,
            start: 0,
        }
    }

    async fn get_token_count(&self, input: &str) -> Result<usize, EmbeddingError> {
        let tokenizer = get_tokenizer_instance().await;
        // Encode the text in the document
        let encoding = tokenizer.encode(input, true).unwrap();

        // Extract the encoding length
        Ok(encoding.len())
    }

    pub async fn get_next_chunk(&mut self) -> Option<Result<(&'a str, usize, usize), EmbeddingError>> {
        if self.start >= self.chunkable.len() {
            return None;
        }
        let starting_substr_size = self.max_chunk_size * 4;
        let mut substr_size = starting_substr_size;
        let mut end = cmp::min(self.chunkable.len(), substr_size + self.start);
        while !self.chunkable.is_char_boundary(end) && end > 0 {
            end -= 1;
        }

        let mut substr = &self.chunkable[self.start..end];
        log::debug!("Getting token count");
        let mut token_count = match self.get_token_count(substr).await {
            Ok(count) => count,
            Err(e) => return Some(Err(e)),
        };
        log::debug!("Got token count");

        while token_count > self.max_chunk_size && substr_size > 0 {
            let mut end = cmp::min(self.chunkable.len(), substr_size + self.start);
            if end < self.chunkable.len() {
                while !self.chunkable.is_char_boundary(end) {
                    end -= 1;
                }
            }

            substr = &self.chunkable[self.start..end];
            substr = if let Some((index, _)) = substr.rmatch_indices('.').next() {
                let max_distance = 50;
                let char_distance = substr[index..].chars().count() - 1;
                if char_distance <= max_distance && char_distance > 10 {
                    &self.chunkable[self.start..self.start + index + 1]
                } else {
                    substr
                }
            } else {
                substr
            };

            token_count = match self.get_token_count(substr).await {
                Ok(count) => count,
                Err(e) => return Some(Err(e)),
            };

            let distance_from_limit = token_count.saturating_sub(self.max_chunk_size);
            substr_size = substr_size.saturating_sub(cmp::max(distance_from_limit, 1));
        }

        let new_end = self.start + substr.len();
        let result = (substr, self.start, new_end);
        self.start = new_end + 1;
        while self.start < self.chunkable.len() && !self.chunkable.is_char_boundary(self.start) {
            self.start += 1;
        }

        Some(Ok(result))
    }
}

impl<'a> Stream for StringChunkIterator<'a> {
    type Item = Result<(&'a str, usize, usize), EmbeddingError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let fut = self.get_next_chunk();
        let mut fut = Box::pin(fut);
        futures::future::Future::poll(fut.as_mut(), cx)
    }
}
