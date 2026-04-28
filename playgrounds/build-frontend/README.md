# Frontend Build Playground

This directory serves as a test fixture for validating frontend plugin build behavior in `@caido-community/dev`.

## Purpose

It demonstrates a minimal frontend Caido plugin configuration and is used in integration tests to verify:

- Vite-based frontend builds
- Asset bundling
- README and referenced asset inclusion in final plugin packages

## Test Asset Reference

The following image reference is used to test automatic asset detection from README content:

![Test Asset](assets/test.png)

## Link Test Cases

Local link that should be preserved:

[Local Doc](assets/test.txt)

External link that should be removed:

[External Link](https://example.com/docs)

Fragment-only link that should be preserved:

[Jump to section](#purpose)

## Reference-Style Definition Test Cases

Reference-style image and link using definitions:

![Ref Image][ref-image]
[Ref Link][ref-link]

[ref-image]: assets/test.png
[ref-link]: https://example.com/docs

## HTML Test Cases

Raw HTML with local and external URLs:

<img src="assets/test.png" alt="HTML Local Image" />
<img src="https://example.com/image.png" alt="HTML External Image" />
<a href="assets/test.txt">HTML Local Link</a>
<a href="https://example.com/docs">HTML External Link</a>

## External URL Test Cases

The following images test external URL handling (should be removed):

![External HTTP](http://example.com/image.png)
![External HTTPS](https://example.com/image.png)
![Data URI](data:image/png;base64,abc123)
