pipeline {
    agent any
    options { 
        buildDiscarder logRotator(artifactDaysToKeepStr: '', artifactNumToKeepStr: '', daysToKeepStr: '', numToKeepStr: '5')
        disableConcurrentBuilds()
        parallelsAlwaysFailFast()
    }
    stages {
        stage('Started') {
            steps {
                echo """
                Job name: ${JOB_NAME}
                Git branch: ${GIT_BRANCH}
                Git commit: ${GIT_COMMIT}
                Git URL: ${GIT_URL}
                """
            }
        }
    }
}