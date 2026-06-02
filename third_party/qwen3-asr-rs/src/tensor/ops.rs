// ---------------------------------------------------------------------------
// Operator overloads (both backends)
// ---------------------------------------------------------------------------

use super::Tensor;

// Add: Tensor + Tensor
impl std::ops::Add<&Tensor> for &Tensor {
    type Output = Tensor;
    fn add(self, rhs: &Tensor) -> Tensor {
        #[cfg(feature = "tch-backend")]
        {
            Tensor::from_tch(&self.inner + &rhs.inner)
        }
        #[cfg(feature = "mlx")]
        {
            Tensor::from_mlx(crate::backend::mlx::ops::add(&self.inner, &rhs.inner))
        }
    }
}

impl std::ops::Add<Tensor> for &Tensor {
    type Output = Tensor;
    fn add(self, rhs: Tensor) -> Tensor {
        self + &rhs
    }
}

impl std::ops::Add<&Tensor> for Tensor {
    type Output = Tensor;
    fn add(self, rhs: &Tensor) -> Tensor {
        &self + rhs
    }
}

impl std::ops::Add<Tensor> for Tensor {
    type Output = Tensor;
    fn add(self, rhs: Tensor) -> Tensor {
        &self + &rhs
    }
}

// Add: Tensor + f64
impl std::ops::Add<f64> for &Tensor {
    type Output = Tensor;
    fn add(self, rhs: f64) -> Tensor {
        #[cfg(feature = "tch-backend")]
        {
            Tensor::from_tch(&self.inner + rhs)
        }
        #[cfg(feature = "mlx")]
        {
            let scalar = crate::backend::mlx::array::MlxArray::scalar_f32(rhs as f32);
            Tensor::from_mlx(crate::backend::mlx::ops::add(&self.inner, &scalar))
        }
    }
}

impl std::ops::Add<f64> for Tensor {
    type Output = Tensor;
    fn add(self, rhs: f64) -> Tensor {
        &self + rhs
    }
}

// Sub: Tensor - Tensor
impl std::ops::Sub<&Tensor> for &Tensor {
    type Output = Tensor;
    fn sub(self, rhs: &Tensor) -> Tensor {
        #[cfg(feature = "tch-backend")]
        {
            Tensor::from_tch(&self.inner - &rhs.inner)
        }
        #[cfg(feature = "mlx")]
        {
            Tensor::from_mlx(crate::backend::mlx::ops::subtract(&self.inner, &rhs.inner))
        }
    }
}

impl std::ops::Sub<Tensor> for &Tensor {
    type Output = Tensor;
    fn sub(self, rhs: Tensor) -> Tensor {
        self - &rhs
    }
}

impl std::ops::Sub<&Tensor> for Tensor {
    type Output = Tensor;
    fn sub(self, rhs: &Tensor) -> Tensor {
        &self - rhs
    }
}

impl std::ops::Sub<Tensor> for Tensor {
    type Output = Tensor;
    fn sub(self, rhs: Tensor) -> Tensor {
        &self - &rhs
    }
}

impl std::ops::Sub<f64> for &Tensor {
    type Output = Tensor;
    fn sub(self, rhs: f64) -> Tensor {
        #[cfg(feature = "tch-backend")]
        {
            Tensor::from_tch(&self.inner - rhs)
        }
        #[cfg(feature = "mlx")]
        {
            let scalar = crate::backend::mlx::array::MlxArray::scalar_f32(rhs as f32);
            Tensor::from_mlx(crate::backend::mlx::ops::subtract(&self.inner, &scalar))
        }
    }
}

// Mul: Tensor * Tensor
impl std::ops::Mul<&Tensor> for &Tensor {
    type Output = Tensor;
    fn mul(self, rhs: &Tensor) -> Tensor {
        #[cfg(feature = "tch-backend")]
        {
            Tensor::from_tch(&self.inner * &rhs.inner)
        }
        #[cfg(feature = "mlx")]
        {
            Tensor::from_mlx(crate::backend::mlx::ops::multiply(&self.inner, &rhs.inner))
        }
    }
}

impl std::ops::Mul<Tensor> for &Tensor {
    type Output = Tensor;
    fn mul(self, rhs: Tensor) -> Tensor {
        self * &rhs
    }
}

impl std::ops::Mul<&Tensor> for Tensor {
    type Output = Tensor;
    fn mul(self, rhs: &Tensor) -> Tensor {
        &self * rhs
    }
}

impl std::ops::Mul<Tensor> for Tensor {
    type Output = Tensor;
    fn mul(self, rhs: Tensor) -> Tensor {
        &self * &rhs
    }
}

// Mul: Tensor * f64
impl std::ops::Mul<f64> for &Tensor {
    type Output = Tensor;
    fn mul(self, rhs: f64) -> Tensor {
        #[cfg(feature = "tch-backend")]
        {
            Tensor::from_tch(&self.inner * rhs)
        }
        #[cfg(feature = "mlx")]
        {
            let scalar = crate::backend::mlx::array::MlxArray::scalar_f32(rhs as f32);
            Tensor::from_mlx(crate::backend::mlx::ops::multiply(&self.inner, &scalar))
        }
    }
}

impl std::ops::Mul<f64> for Tensor {
    type Output = Tensor;
    fn mul(self, rhs: f64) -> Tensor {
        &self * rhs
    }
}

// Div: Tensor / Tensor
impl std::ops::Div<&Tensor> for &Tensor {
    type Output = Tensor;
    fn div(self, rhs: &Tensor) -> Tensor {
        #[cfg(feature = "tch-backend")]
        {
            Tensor::from_tch(&self.inner / &rhs.inner)
        }
        #[cfg(feature = "mlx")]
        {
            Tensor::from_mlx(crate::backend::mlx::ops::divide(&self.inner, &rhs.inner))
        }
    }
}

impl std::ops::Div<Tensor> for &Tensor {
    type Output = Tensor;
    fn div(self, rhs: Tensor) -> Tensor {
        self / &rhs
    }
}

impl std::ops::Div<&Tensor> for Tensor {
    type Output = Tensor;
    fn div(self, rhs: &Tensor) -> Tensor {
        &self / rhs
    }
}

impl std::ops::Div<Tensor> for Tensor {
    type Output = Tensor;
    fn div(self, rhs: Tensor) -> Tensor {
        &self / &rhs
    }
}

// Div: Tensor / f64
impl std::ops::Div<f64> for &Tensor {
    type Output = Tensor;
    fn div(self, rhs: f64) -> Tensor {
        #[cfg(feature = "tch-backend")]
        {
            Tensor::from_tch(&self.inner / rhs)
        }
        #[cfg(feature = "mlx")]
        {
            let scalar = crate::backend::mlx::array::MlxArray::scalar_f32(rhs as f32);
            Tensor::from_mlx(crate::backend::mlx::ops::divide(&self.inner, &scalar))
        }
    }
}

impl std::ops::Div<f64> for Tensor {
    type Output = Tensor;
    fn div(self, rhs: f64) -> Tensor {
        &self / rhs
    }
}

// Neg: -Tensor
impl std::ops::Neg for &Tensor {
    type Output = Tensor;
    fn neg(self) -> Tensor {
        #[cfg(feature = "tch-backend")]
        {
            Tensor::from_tch(-&self.inner)
        }
        #[cfg(feature = "mlx")]
        {
            Tensor::from_mlx(crate::backend::mlx::ops::negative(&self.inner))
        }
    }
}

impl std::ops::Neg for Tensor {
    type Output = Tensor;
    fn neg(self) -> Tensor {
        -&self
    }
}

// AddAssign
impl std::ops::AddAssign<&Tensor> for Tensor {
    fn add_assign(&mut self, rhs: &Tensor) {
        *self = &*self + rhs;
    }
}

impl std::ops::AddAssign<Tensor> for Tensor {
    fn add_assign(&mut self, rhs: Tensor) {
        *self = &*self + &rhs;
    }
}
