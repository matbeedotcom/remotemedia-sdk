use remotemedia_runtime::python::multiprocess::data_transfer::RuntimeData;

fn main() {
    let tiny = RuntimeData::text("Hi", "s");
    let bytes = tiny.to_bytes();
    println!("'Hi' payload serialized size: {} bytes", bytes.len());
    
    let small = RuntimeData::text("x", "s");
    let bytes2 = small.to_bytes();
    println!("'x' payload serialized size: {} bytes", bytes2.len());
    
    let kb = RuntimeData {
        data_type: remotemedia_runtime::python::multiprocess::data_transfer::DataType::Audio,
        session_id: "s".to_string(),
        timestamp: 12345,
        payload: vec![0u8; 1024],
    };
    let bytes3 = kb.to_bytes();
    println!("1KB payload serialized size: {} bytes", bytes3.len());
}
