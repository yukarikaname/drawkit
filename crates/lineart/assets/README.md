# TEED

`teed.onnx` is the 58K-parameter TEED edge detector from
[Shaboomi/teed-onnx](https://huggingface.co/Shaboomi/teed-onnx), converted from the
upstream `xavysp/TEED` BIPED checkpoint. Upstream code and this export are MIT; see
`TEED-LICENSE`.

Input is BGR, 0-255, with the mean `(104.007, 116.669, 122.679)` subtracted; the
height and width must be multiples of 8. Output is a single edge probability map.
The weights were released with the BIPED research dataset; verify the dataset terms
before commercial redistribution.
