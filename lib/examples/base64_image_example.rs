use shiva::core::{ImageData, ImageDimension};

fn main() -> anyhow::Result<()> {
    // Example: Create an ImageData element from Base64 data
    // This is a small 1x1 PNG image encoded in Base64
    let base64_image = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==";
    
    // Create ImageData from base64
    let image = ImageData::from_base64(
        base64_image,
        "Sample Image".to_string(),
        "A 1x1 transparent PNG".to_string(),
        "png".to_string(),
        "center".to_string(),
        ImageDimension {
            width: Some("100px".to_string()),
            height: Some("100px".to_string()),
        },
    )?;

    println!("Created ImageData from Base64:");
    println!("  Title: {}", image.title());
    println!("  Alt text: {}", image.alt());
    println!("  Image type: {:?}", image.image_type());
    println!("  Alignment: {:?}", image.align());
    println!("  Size: {:?}", image.size());
    println!("  Bytes length: {}", image.bytes().len());

    // Convert back to base64
    let encoded_back = image.to_base64();
    println!("\nRound-trip test:");
    println!("  Original matches encoded: {}", base64_image == encoded_back);

    Ok(())
}
