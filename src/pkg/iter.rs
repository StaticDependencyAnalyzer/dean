use rayon::iter::ParallelIterator;

pub trait ToSequential<T>
where
    T: ParallelIterator,
{
    fn to_seq(self, buffer_size: usize) -> ParallelToSequential<T>;
}

impl<T> ToSequential<T> for T
where
    T: ParallelIterator + 'static,
{
    fn to_seq(self, buffer_size: usize) -> ParallelToSequential<T> {
        ParallelToSequential::new(self, buffer_size)
    }
}

/// An iterator that transforms a `ParallelIterator` from rayon, into a standard sequential Iterator.
pub struct ParallelToSequential<T>
where
    T: ParallelIterator,
{
    channel: std::sync::mpsc::Receiver<T::Item>,
    join_handle: Option<std::thread::JoinHandle<()>>,
}

impl<T> Drop for ParallelToSequential<T>
where
    T: ParallelIterator,
{
    fn drop(&mut self) {
        self.join_handle.take().unwrap().join().unwrap();
    }
}

impl<T> ParallelToSequential<T>
where
    T: ParallelIterator + 'static,
{
    fn new(parallel: T, size: usize) -> Self {
        let (sender, receiver) = std::sync::mpsc::sync_channel(size);

        let join_handle = std::thread::spawn(move || {
            parallel.for_each(|item| {
                #[allow(clippy::let_underscore_drop)]
                let _ = sender.send(item);
            });
        });

        ParallelToSequential {
            channel: receiver,
            join_handle: Some(join_handle),
        }
    }
}

impl<T> Iterator for ParallelToSequential<T>
where
    T: ParallelIterator,
{
    type Item = T::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.channel.recv().ok()
    }
}
