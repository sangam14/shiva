use shiva::core::{ImageData, ImageDimension};
use std::fs;

fn main() -> anyhow::Result<()> {
    // Read the image file
    let image_path = "test/data/big_image1.png";
    
    println!("Reading image from: {}", image_path);
    
    // Read the file bytes
    let image_bytes = fs::read(image_path)?;
    println!("Image size: {} bytes", image_bytes.len());
    
    // Create ImageData from the raw bytes
    let image_data = ImageData::new(
        bytes::Bytes::from(image_bytes),
        "Big Image 1".to_string(),
        "A large PNG image for testing".to_string(),
        "png".to_string(),
        "center".to_string(),
        ImageDimension {
            width: Some("800px".to_string()),
            height: Some("600px".to_string()),
        },
    );
    
    // Convert to Base64
    println!("Converting to Base64...");
    let base64_string = image_data.to_base64();
    
    // Show first and last 100 characters of Base64 string
    let base64_len = base64_string.len();
    println!("Base64 string length: {} characters", base64_len);
    println!("First 100 characters: {}", &base64_string[..100.min(base64_len)]);
    if base64_len > 100 {
        println!("Last 100 characters: {}", &base64_string[base64_len.saturating_sub(100)..]);
    }
    
    // Test round-trip: convert back from Base64
    println!("\nTesting round-trip conversion...");
    let restored_image = ImageData::from_base64(
        &base64_string,
        "Restored Image".to_string(),
        "Image restored from Base64".to_string(),
        "png".to_string(),
        "center".to_string(),
        ImageDimension::default(),
    )?;
    
    println!("Original image bytes: {}", image_data.bytes().len());
    println!("Restored image bytes: {}", restored_image.bytes().len());
    println!("Round-trip successful: {}", image_data.bytes() == restored_image.bytes());
    
    // Optionally, save the full Base64 string to a file
    println!("\nSaving Base64 string to file...");
    fs::write("big_image1_base64.txt", &base64_string)?;
    println!("Base64 string saved to: big_image1_base64.txt");
    
    // Create a data URL format (common for web use)
    let data_url = format!("data:image/png;base64,{}", base64_string);
    println!("Data URL length: {} characters", data_url.len());
    println!("Data URL prefix: {}", &data_url[..50.min(data_url.len())]);
    
    Ok(())
}
