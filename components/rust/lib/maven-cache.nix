{
  pkgs,
  repositoryUrl,
}: let
  insecure = builtins.match "^http://.*" repositoryUrl != null;
in
  pkgs.writeText "harbor-maven-cache.init.gradle" ''
    import java.net.URI
    import org.gradle.api.artifacts.repositories.MavenArtifactRepository
    import org.gradle.api.initialization.resolve.RepositoriesMode

    def proxyUrl = URI.create(${builtins.toJSON repositoryUrl})
    def publicRepositoryUrls = [
      "https://repo.maven.apache.org/maven2",
      "https://repo.maven.apache.org/maven2/",
      "https://repo1.maven.org/maven2",
      "https://repo1.maven.org/maven2/",
      "https://dl.google.com/dl/android/maven2",
      "https://dl.google.com/dl/android/maven2/",
      "https://maven.google.com",
      "https://maven.google.com/",
      "https://plugins.gradle.org/m2",
      "https://plugins.gradle.org/m2/"
    ] as Set

    def routeThroughProxy = { repositories ->
      repositories.removeAll { repository ->
        repository instanceof MavenArtifactRepository &&
          publicRepositoryUrls.contains(repository.url.toString())
      }
      if (repositories.findByName("harborMavenCache") == null) {
        repositories.maven {
          name = "harborMavenCache"
          url = proxyUrl
          allowInsecureProtocol = ${
      if insecure
      then "true"
      else "false"
    }
        }
      }
    }

    gradle.beforeSettings { settings ->
      routeThroughProxy(settings.pluginManagement.repositories)
      routeThroughProxy(settings.dependencyResolutionManagement.repositories)
    }

    gradle.settingsEvaluated { settings ->
      routeThroughProxy(settings.pluginManagement.repositories)
      routeThroughProxy(settings.dependencyResolutionManagement.repositories)
    }

    gradle.beforeProject { project ->
      routeThroughProxy(project.buildscript.repositories)
    }

    gradle.afterProject { project, state ->
      routeThroughProxy(project.buildscript.repositories)
      if (project.gradle.settings.dependencyResolutionManagement.repositoriesMode.get() == RepositoriesMode.PREFER_PROJECT) {
        routeThroughProxy(project.repositories)
      }
    }
  ''
