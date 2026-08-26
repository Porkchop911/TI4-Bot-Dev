//! Which device is linked, and can the optimizer path actually compute on it?
fn main() {
    let backend = ti4_tensor::backend();
    println!("cuda available   {}", backend.cuda);
    println!("cuda devices     {}", backend.cuda_devices);
    println!("inference device {:?}", ti4_tensor::inference_device());
    match ti4_tensor::OptimizerDevice::Cuda.resolve() {
        Ok(device) => {
            println!("optimizer cuda   resolves to {device:?}");
            let a = ti4_tensor::Tensor::from_slice(&[1.0f32, 2.0, 3.0]).to_device(device);
            let b = (&a * 2.0).sum(ti4_tensor::Kind::Float);
            println!("gpu arithmetic   sum(2x) = {}", b.double_value(&[]));
            println!("result device    {:?}", b.device());
        }
        Err(error) => println!("optimizer cuda   refused: {error}"),
    }
}
