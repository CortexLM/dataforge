# Le Problème: Tasks Théoriques vs Environnements Réels

## Situation Actuelle

Les tasks générées par `dataforge generate` sont des **énoncés de problèmes** sans environnement réel:

```
Task: "Diagnostiquer pourquoi les pods K8s sont evicted pour inode exhaustion"
     ↓
Problème: On n'a PAS de cluster Kubernetes avec le problème réel !
```

## Pourquoi c'est comme ça ?

1. **Dataforge génère des benchmarks** - L'objectif est de créer des énoncés de qualité pour évaluer des agents
2. **Les environnements complexes sont coûteux** - Créer un vrai cluster K8s cassé demande beaucoup de ressources
3. **C'est conçu pour s'intégrer avec d'autres outils** - SWE-Bench, des harnesses d'évaluation existants

## Solutions Possibles

### Option A: Tasks Simples avec Fichiers Générés (Réalisable maintenant)

Dataforge a des générateurs de fichiers (logs, configs, CSV). On peut créer des tasks qui:
- Analysent des fichiers de logs générés
- Trouvent des erreurs dans des configs
- Traitent des données CSV

**Exemple réalisable:**
```yaml
Task: "Trouver l'erreur dans le fichier de log"
Environnement: Un fichier app.log de 1000 lignes avec une erreur à la ligne 500
Vérification: L'agent trouve "ERROR 503 at line 500"
```

### Option B: Utiliser des Templates Existants

Les templates dans `examples/templates/` sont plus simples:
- `file-search-easy-001.yaml` - Recherche de fichiers
- `text-processing-easy-001.yaml` - Traitement de texte
- `log-analysis-001.yaml` - Analyse de logs

### Option C: Intégration avec SWE-Bench (Avancé)

SWE-Bench fournit de vrais repos GitHub avec de vrais bugs. Dataforge peut s'y connecter via le module `collectors/swe_bench.rs`.

### Option D: Docker Compose pour Environnements Simples

Pour des tasks de niveau "file-operations" ou "debugging" simple:

```yaml
# docker-compose.yaml
services:
  task-env:
    image: ubuntu:24.04
    volumes:
      - ./generated_files:/data
    command: sleep infinity
```

## Recommandation Immédiate

**Pour avoir des tasks VRAIMENT vérifiables maintenant:**

1. Utiliser les templates simples existants
2. Générer avec la catégorie `file-operations` ou `debugging`
3. Les fichiers seront générés ET vérifiables

```bash
# Générer une task simple avec fichiers
dataforge generate \
  --count 1 \
  --category file-operations \
  --model moonshotai/kimi-k2.5 \
  --validate-docker \
  --output ./simple-tasks
```

## Ce qu'il faudrait ajouter à Dataforge

Pour avoir de vraies tasks vérifiables sur des scénarios complexes:

1. **Environment Provisioners** - Scripts qui créent l'état "cassé"
2. **State Validators** - Vérification que le problème est résolu (pas juste que des fichiers existent)
3. **Simulation Containers** - Mini-environnements qui simulent les problèmes

Exemple de ce qui manque:
```yaml
# Ce qu'on voudrait dans task.yaml
environment_setup:
  - action: create_file
    path: /var/log/app.log
    generator: log_file
    config:
      lines: 1000
      inject_error_at: 500
      error_type: "connection_timeout"

verification:
  - type: state_check
    description: "Error has been identified"
    command: "cat /workspace/answer.txt"
    expected_contains: "line 500"
```
