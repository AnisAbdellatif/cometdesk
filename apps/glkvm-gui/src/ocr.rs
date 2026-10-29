//! Text from a screen image: Apple Vision, run locally. The image is a PNG from the agent
//! (pixel-exact) or the device's snapshot (JPEG of the capture, before H.264).

/// All recognized text, top to bottom, one line per Vision observation.
#[cfg(target_os = "macos")]
pub fn recognize(image: &[u8]) -> Result<String, String> {
    use objc2::AnyThread;
    use objc2::rc::Retained;
    use objc2_foundation::{NSArray, NSData, NSDictionary};
    use objc2_vision::{
        VNImageRequestHandler, VNRecognizeTextRequest, VNRecognizedTextObservation, VNRequest,
    };

    let data = NSData::with_bytes(image);
    let handler = VNImageRequestHandler::initWithData_options(
        VNImageRequestHandler::alloc(),
        &data,
        &NSDictionary::new(),
    );
    let request = VNRecognizeTextRequest::new();
    let as_request: Retained<VNRequest> =
        Retained::into_super(Retained::into_super(request.clone()));
    handler
        .performRequests_error(&NSArray::from_retained_slice(&[as_request]))
        .map_err(|e| e.localizedDescription().to_string())?;
    let Some(results) = request.results() else {
        return Ok(String::new());
    };
    let mut lines: Vec<(f64, f64, String)> = Vec::new();
    for obs in results.iter() {
        let Ok(obs) = obs.downcast::<VNRecognizedTextObservation>() else {
            continue;
        };
        let Some(best) = obs.topCandidates(1).iter().next() else {
            continue;
        };
        // Vision's origin is bottom-left: larger y is higher on the screen.
        let b = unsafe { obs.boundingBox() };
        lines.push((
            b.origin.y + b.size.height,
            b.origin.x,
            best.string().to_string(),
        ));
    }
    lines.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.total_cmp(&b.1)));
    Ok(lines
        .into_iter()
        .map(|l| l.2)
        .collect::<Vec<_>>()
        .join("\n"))
}

#[cfg(not(target_os = "macos"))]
pub fn recognize(_: &[u8]) -> Result<String, String> {
    Err("text recognition needs Apple Vision (macOS)".into())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    /// Reads a PNG rendered with AppKit (`swift` script outside the repo); run with `--ignored`.
    #[test]
    #[ignore]
    fn reads_rendered_text() {
        let path = std::env::var("OCR_TEST_PNG").expect("OCR_TEST_PNG");
        let text = super::recognize(&std::fs::read(&path).unwrap()).unwrap();
        println!("{text}");
        assert!(
            text.contains("GLKVM") && text.contains("192.168.1.5"),
            "{text}"
        );
    }
}
