G_gitcred = 'TonJenSSH'
G_container = "atomxy/empty-ton-sdk-js:20191128"
G_update = "none"
G_build = "none"
G_test = "none"

pipeline {
    triggers {
        upstream(
            upstreamProjects: 'Node/ton-labs-types/master',
            threshold: hudson.model.Result.SUCCESS
        )
    }
    options {
        buildDiscarder logRotator(artifactDaysToKeepStr: '', artifactNumToKeepStr: '', daysToKeepStr: '', numToKeepStr: '1')
        disableConcurrentBuilds()
        parallelsAlwaysFailFast()
    }
    agent {
        docker {
            image G_container
        }
    }
    stages {
        stage('Collect commit data') {
            steps {
                sshagent([G_gitcred]) {
                    script {
                        C_PROJECT = env.GIT_URL.substring(15, env.GIT_URL.length() - 4)
                        C_COMMITER = sh (script: 'git show -s --format=%cn ${GIT_COMMIT}', returnStdout: true).trim()
                        C_TEXT = sh (script: 'git show -s --format=%s ${GIT_COMMIT}', returnStdout: true).trim()
                        C_AUTHOR = sh (script: 'git show -s --format=%an ${GIT_COMMIT}', returnStdout: true).trim()
                        C_HASH = sh (script: 'git show -s --format=%h ${GIT_COMMIT}', returnStdout: true).trim()
                    
                        DiscordURL = "https://discordapp.com/api/webhooks/496992026932543489/4exQIw18D4U_4T0H76bS3Voui4SyD7yCQzLP9IRQHKpwGRJK1-IFnyZLyYzDmcBKFTJw"
                        string DiscordFooter = "Build duration is ${currentBuild.durationString}"
                        DiscordTitle = "Job ${JOB_NAME} from GitHub ${C_PROJECT}"
                    }
                }
            }
        }
        stage('Update') {
            steps {
                sshagent([G_gitcred]) {
                    sh 'cargo clean'
                    sh 'cargo update'
                }
            }
            post {
                success { script { G_update = "success" } }
                failure { script { G_update = "failure" } }
            }
        }
        stage('Build') {
            steps {
                sshagent([G_gitcred]) {
                    sh 'cargo build --release'
                }
            }
            post {
                success { script { G_build = "success" } }
                failure { script { G_build = "failure" } }
            }
        }
        stage('Tests') {
            steps {
                sh 'cargo test --release'
            }
            post {
                success { script { G_test = "success" } }
                failure { script { G_test = "failure" } }
            }
        }
    }
    post {
        always {
            node('master') {
                script {
                    DiscordDescription = """${C_COMMITER} pushed commit ${C_HASH} by ${C_AUTHOR} with a message '${C_TEXT}'
Build number ${BUILD_NUMBER}
Update: **${G_update}** 
Build: **${G_build}**
Tests: **${G_test}**"""
                    
                discordSend(
                    title: DiscordTitle, 
                    description: DiscordDescription, 
                    footer: DiscordFooter, 
                    link: RUN_DISPLAY_URL, 
                    successful: currentBuild.resultIsBetterOrEqualTo('SUCCESS'), 
                    webhookURL: DiscordURL
                )
                }
            } 
        }
    }
}