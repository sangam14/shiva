use bytes::Bytes;
use clap::{Parser, ValueHint};
use shiva::core::{Document, DocumentType, ImageData, ImageDimension};
use std::path::Path;

#[derive(Parser, Debug)]
#[command(
    name = "shiva",
    author,
    version,
    about = "CLI Shiva: Converting documents from any format to any",
    long_about = None
)]
struct Args {
    #[arg(
        value_name = "INPUT_FILE",
        help = &format!(
            "Input file (possible formats: {})",
            DocumentType::supported_extensions().join(", ")
        ),
        value_hint = ValueHint::FilePath,
        required_unless_present = "image_to_base64"
    )]
    input_file: Option<String>,

    #[arg(
        value_name = "OUTPUT_FILE",
        help = &format!(
            "Output file (possible formats: {})",
            DocumentType::supported_extensions().join(", ")
        ),
        value_hint = ValueHint::FilePath,
        required_unless_present = "image_to_base64"
    )]
    output_file: Option<String>,

    #[arg(
        long = "base64-images",
        help = "Convert images to Base64 format in the output"
    )]
    base64_images: bool,

    #[arg(
        long = "image-to-base64",
        help = "Convert a single image file to Base64 format",
        value_hint = ValueHint::FilePath
    )]
    image_to_base64: Option<String>,

    #[arg(
        short = 'o',
        long = "output",
        help = "Output file for Base64 conversion (when using --image-to-base64)",
        value_hint = ValueHint::FilePath,
        required_if_eq("image_to_base64", "true")
    )]
    base64_output: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Handle single image to Base64 conversion
    if let Some(image_path) = &args.image_to_base64 {
        let output_path = args.base64_output.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Output file is required when using --image-to-base64")
        })?;
        return convert_image_to_base64(image_path, output_path);
    }

    // Handle regular document conversion
    let input_file = args.input_file.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Input file is required for document conversion")
    })?;
    let output_file = args.output_file.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Output file is required for document conversion")
    })?;

    let input_path = Path::new(input_file);
    let output_path = Path::new(output_file);

    let supported_formats = DocumentType::supported_extensions();

    let input_format = match input_path.extension() {
        Some(ext) => ext.to_str().ok_or_else(|| {
            anyhow::anyhow!(
                "Invalid extension of the input file. Supported formats are: {}",
                supported_formats.join(", ")
            )
        })?,
        None => {
            return Err(anyhow::anyhow!(
                "The input file has no extension. Supported formats are: {}",
                supported_formats.join(", ")
            ))
        }
    };

    let output_format = match output_path.extension() {
        Some(ext) => ext.to_str().ok_or_else(|| {
            anyhow::anyhow!(
                "Invalid output file extension. Supported formats are: {}",
                supported_formats.join(", ")
            )
        })?,
        None => {
            return Err(anyhow::anyhow!(
                "The output file has no extension. Supported formats are: {}",
                supported_formats.join(", ")
            ))
        }
    };

    let input_doc_type = DocumentType::from_extension(input_format).ok_or_else(|| {
        anyhow::anyhow!(
            "Unsupported input file format '{}'. Supported formats are: {}",
            input_format,
            supported_formats.join(", ")
        )
    })?;

    let output_doc_type = DocumentType::from_extension(output_format).ok_or_else(|| {
        anyhow::anyhow!(
            "Unsupported output file format '{}'. Supported formats are: {}",
            output_format,
            supported_formats.join(", ")
        )
    })?;

    let input_vec = std::fs::read(input_file)?;
    let input_bytes = Bytes::from(input_vec);

    let document = Document::parse(&input_bytes, input_doc_type)?;
    let output = if args.base64_images && output_format == "md" {
        // Use custom image saver to trigger Base64 embedding
        document.generate_with_saver(output_doc_type, |_, marker| {
            if marker == "__base64__" {
                Ok(()) // Signal to embed as Base64
            } else {
                // Default: save image
                std::fs::write(marker, &[])?;
                Ok(())
            }
        })?
    } else {
        document.generate(output_doc_type)?
    };
    std::fs::write(output_file, output)?;
    if args.base64_images && output_format == "md" {
        println!("Document converted with Base64 images embedded in markdown.");
    } else {
        println!("Document converted successfully");
    }

    Ok(())
}

/// Convert a single image file to Base64 format
fn convert_image_to_base64(image_path: &str, output_path: &str) -> anyhow::Result<()> {
    use std::fs;
    
    println!("Converting image to Base64: {}", image_path);
    
    // Read the image file
    let image_bytes = fs::read(image_path)?;
    println!("Image size: {} bytes", image_bytes.len());
    
    // Determine image type from extension
    let image_type = Path::new(image_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("png")
        .to_string();
    
    // Create ImageData from the raw bytes
    let image_data = ImageData::new(
        Bytes::from(image_bytes),
        "Image".to_string(),
        "Converted image".to_string(),
        image_type.clone(),
        "center".to_string(),
        ImageDimension::default(),
    );
    
    // Convert to Base64
    let base64_string = image_data.to_base64();
    println!("Base64 string length: {} characters", base64_string.len());
    
    // Determine output format
    let output_ext = Path::new(output_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("txt");
        
    match output_ext {
        "txt" => {
            // Save as plain Base64 text
            fs::write(output_path, &base64_string)?;
            println!("Base64 string saved to: {}", output_path);
        }
        "md" => {
            // Save as Markdown with embedded image
            let markdown_content = format!(
                "# Converted Image\n\n![{}](data:image/{};base64,{})\n\nBase64 Data:\n```\n{}\n```\n",
                Path::new(image_path).file_name().unwrap_or_default().to_string_lossy(),
                image_type,
                base64_string,
                base64_string
            );
            fs::write(output_path, markdown_content)?;
            println!("Markdown with Base64 image saved to: {}", output_path);
        }
        "html" => {
            // Save as HTML with embedded image
            let html_content = format!(
                r#"<!DOCTYPE html>
<html>
<head>
    <title>Converted Image</title>
</head>
<body>
    <h1>Converted Image</h1>
    <img src="data:image/{};base64,{}" alt="{}" style="max-width: 100%; height: auto;">
    
    <h2>Base64 Data</h2>
    <textarea rows="10" cols="80" readonly>{}</textarea>
</body>
</html>"#,
                image_type,
                base64_string,
                Path::new(image_path).file_name().unwrap_or_default().to_string_lossy(),
                base64_string
            );
            fs::write(output_path, html_content)?;
            println!("HTML with Base64 image saved to: {}", output_path);
        }
        "json" => {
            // Save as JSON
            let json_content = format!(
                r#"{{
  "image": {{
    "filename": "{}",
    "type": "{}",
    "size_bytes": {},
    "base64": "{}",
    "data_url": "data:image/{};base64,{}"
  }}
}}"#,
                Path::new(image_path).file_name().unwrap_or_default().to_string_lossy(),
                image_type,
                image_data.bytes().len(),
                base64_string,
                image_type,
                base64_string
            );
            fs::write(output_path, json_content)?;
            println!("JSON with Base64 image saved to: {}", output_path);
        }
        _ => {
            // Default to plain text
            fs::write(output_path, &base64_string)?;
            println!("Base64 string saved to: {}", output_path);
        }
    }
    
    // Show preview
    let preview_len = 100.min(base64_string.len());
    println!("Base64 preview: {}...", &base64_string[..preview_len]);
    
    Ok(())
}
