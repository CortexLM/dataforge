import pathlib
import yaml


def load_recipe():
    recipe_path = pathlib.Path("recipe/recipe.yaml")
    return yaml.safe_load(recipe_path.read_text())


def test_recipe_structure():
    data = load_recipe()

    assert data["package"]["name"] == "${{ name|lower }}"
    assert data["build"]["noarch"] == "python"

    run_reqs = data["requirements"]["run"]
    assert "requests >=2.20" in run_reqs
    assert "typing_extensions >=4.0.0" in run_reqs

    test_config = data["tests"][0]["python"]
    assert test_config["pip_check"] is True
    assert "elevenlabs" in test_config["imports"]
