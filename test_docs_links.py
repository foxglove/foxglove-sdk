#!/usr/bin/env python3
"""Test that all markdown links resolve correctly."""
import re
from pathlib import Path

def test_cpp_readme_rgb_camera_link():
    """Test that cpp/README.md correctly links to rgb-camera-visualization README."""
    readme_path = Path("cpp/README.md")
    content = readme_path.read_text()

    # Find the link to rgb-camera-visualization
    match = re.search(r'\[example\'s readme\]\(([^)]+)\)', content)
    assert match, "Could not find 'example's readme' link"

    link_target = match.group(1)

    # The link should be relative to cpp/README.md
    # So it should be "examples/rgb-camera-visualization/README.md"
    expected = "examples/rgb-camera-visualization/README.md"
    assert link_target == expected, f"Link should be '{expected}' but is '{link_target}'"

    # Verify the target exists
    target_path = readme_path.parent / link_target
    assert target_path.exists(), f"Link target does not exist: {target_path}"

def test_rgb_camera_readme_markdown():
    """Test that rgb-camera-visualization README has proper markdown formatting."""
    readme_path = Path("cpp/examples/rgb-camera-visualization/README.md")
    lines = readme_path.read_text().splitlines()

    # Check line 9 - should be "**Ubuntu/Debian:**"
    assert lines[8] == "**Ubuntu/Debian:**", f"Line 9 formatting incorrect: {lines[8]}"

if __name__ == "__main__":
    test_cpp_readme_rgb_camera_link()
    test_rgb_camera_readme_markdown()
    print("All tests passed!")
