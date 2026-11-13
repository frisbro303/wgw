//! Named n-dimensional arrays: the one data type that moves between files, widgets and the renderer.

#[derive(Clone, Debug)]
pub enum Data {
    F32(Vec<f32>),
    U32(Vec<u32>),
}

/// A flat buffer with a shape, row-major, like a NumPy array. An array with one more axis than its key
/// expects (see `layer::rank`) holds frames along that leading axis.
#[derive(Clone, Debug)]
pub struct NdArray {
    pub data: Data,
    pub shape: Vec<usize>,
}

impl NdArray {
    pub fn new(data: Data, shape: Vec<usize>) -> Result<Self, String> {
        let len = match &data {
            Data::F32(v) => v.len(),
            Data::U32(v) => v.len(),
        };
        let size: usize = shape.iter().product();
        if size != len {
            return Err(format!("shape {:?} needs {} numbers, got {}", shape, size, len));
        }
        Ok(NdArray { data, shape })
    }

    /// Frames along the leading axis, when the array has more axes than `rank`.
    pub fn frames(&self, rank: usize) -> usize {
        if self.shape.len() > rank { self.shape[0].max(1) } else { 1 }
    }

    fn frame_len(&self, rank: usize) -> usize {
        self.shape[self.shape.len().saturating_sub(rank)..].iter().product()
    }

    /// Frame k (clamped to the last) of a float array.
    pub fn f32_frame(&self, rank: usize, k: usize) -> Option<&[f32]> {
        match &self.data {
            Data::F32(v) => {
                let n = self.frame_len(rank);
                let k = k.min(self.frames(rank) - 1);
                v.get(k * n..(k + 1) * n)
            }
            Data::U32(_) => None,
        }
    }

    pub fn u32s(&self) -> Option<&[u32]> {
        match &self.data {
            Data::U32(v) => Some(v),
            Data::F32(_) => None,
        }
    }
}
