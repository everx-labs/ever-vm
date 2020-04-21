G_giturl = ""
G_gitcred = 'TonJenSSH'
G_docker_creds = "TonJenDockerHub"
G_params = null
G_images = [:]
G_docker_image = null
G_commit = ""
G_binversion = "NotSet"

def isUpstream() {
    return currentBuild.getBuildCauses()[0]._class.toString() == 'hudson.model.Cause$UpstreamCause'
}

pipeline {
    tools {nodejs "Node12.8.0"}
    options {
        buildDiscarder logRotator(artifactDaysToKeepStr: '', artifactNumToKeepStr: '', daysToKeepStr: '', numToKeepStr: '10')
        
        parallelsAlwaysFailFast()
    }
    agent {
        node {
            label 'master'
        }
    }
    parameters {
        string(
            name:'common_version',
            defaultValue: '',
            description: 'Common version'
        )
        string(
            name:'image_ton_labs_types',
            defaultValue: '',
            description: 'ton-labs-types image name'
        )
        string(
            name:'image_ton_labs_vm',
            defaultValue: '',
            description: 'ton-labs-vm image name'
        )
    }
    stages {
        stage('Collect commit data') {
            steps {
                sshagent([G_gitcred]) {
                    script {
                        G_giturl = env.GIT_URL
                        echo "${G_giturl}"
                        if(isUpstream() && GIT_BRANCH != "master") {
                            checkout(
                                [$class: 'GitSCM', 
                                branches: [[name: "origin/${GIT_BRANCH}"]], 
                                doGenerateSubmoduleConfigurations: false, 
                                extensions: [[
                                    $class: 'PreBuildMerge', 
                                    options: [
                                        mergeRemote: 'origin', 
                                        mergeTarget: 'master'
                                    ]
                                ]], 
                                submoduleCfg: [], 
                                userRemoteConfigs: [[credentialsId: 'TonJen', url: G_giturl]]])
                            G_commit = sh (script: 'git rev-parse HEAD^{commit}', returnStdout: true).trim()
                            echo "${GIT_COMMIT} merged with master. New commit ${G_commit}"
                        } else {
                            G_commit = GIT_COMMIT
                        }
                        C_PROJECT = env.GIT_URL.substring(19, env.GIT_URL.length() - 4)
                        C_COMMITER = sh (script: 'git show -s --format=%cn ${GIT_COMMIT}', returnStdout: true).trim()
                        C_TEXT = sh (script: 'git show -s --format=%s ${GIT_COMMIT}', returnStdout: true).trim()
                        C_AUTHOR = sh (script: 'git show -s --format=%an ${GIT_COMMIT}', returnStdout: true).trim()
                        C_HASH = sh (script: 'git show -s --format=%h ${GIT_COMMIT}', returnStdout: true).trim()
                    
                        if(params.image_ton_labs_types) {
                            G_images.put('ton-labs-types', params.image_ton_labs_types)
                        } else {
                            G_images.put('ton-labs-types', "tonlabs/ton-labs-types:latest")
                        }
                        if(params.image_ton_labs_vm) {
                            G_images.put('ton-labs-vm', params.image_ton_labs_vm)
                        } else {
                            G_images.put('ton-labs-vm', "tonlabs/ton-labs-vm:source-${G_commit}")
                        }
                        env.IMAGE = G_images['ton-labs-vm']
                        
                        def buildCause = currentBuild.getBuildCauses()[0].shortDescription
                        echo "Build cause: ${buildCause}"
                    }
                }
            }
        }
        stage('Versioning') {
            steps {
                script {
                    lock('bucket') {
                        withAWS(credentials: 'CI_bucket_writer', region: 'eu-central-1') {
                            identity = awsIdentity()
                            s3Download bucket: 'sdkbinaries.tonlabs.io', file: 'version.json', force: true, path: 'version.json'
                        }
                    }
                    if(params.common_version) {
                        G_binversion = sh (script: "node tonVersion.js --set ${params.common_version} .", returnStdout: true).trim()
                    } else {
                        G_binversion = sh (script: "node tonVersion.js .", returnStdout: true).trim()
                    }
                }
            }
        }
        stage('ton-labs-vm') {
            stages {
                stage('Switch to file source') {
                    steps {
                        script {
                            sh """
                                (cat Cargo.toml | sed 's/ton_types = .*/ton_types = { path = \"\\/tonlabs\\/ton-labs-types\" }/g') > tmp.toml
                                rm Cargo.toml
                                mv ./tmp.toml ./Cargo.toml
                            """
                        }
                    }
                }
                stage('Prepare image') {
                    steps {
                        echo "Prepare image..."
                        script {
                            docker.withRegistry('', G_docker_creds) {
                                args = "--pull --no-cache --label 'git-commit=${GIT_COMMIT}' --target ton-labs-vm-src --force-rm ."
                                G_docker_image = docker.build(
                                    G_images['ton-labs-vm'], 
                                    args
                                )
                                echo "Image ${G_docker_image} as ${G_images['ton-labs-vm']}"
                                G_docker_image.push()
                            }
                        }
                    }
                }
                stage('Build') {
                    agent {
                        dockerfile {
                            registryCredentialsId "${G_docker_creds}"
                            additionalBuildArgs "--pull --target ton-labs-vm-rust " + 
                                                "--build-arg \"TON_LABS_TYPES_IMAGE=${G_images['ton-labs-types']}\" " +
                                                "--build-arg \"TON_LABS_VM_IMAGE=${G_images['ton-labs-vm']}\""
                        }
                    }
                    steps {
                        script {
                            sh """
                                cd /tonlabs/ton-labs-vm
                                cargo update
                                cargo build --release
                            """
                        }
                    }
                }
                stage('Tests') {
                    agent {
                        dockerfile {
                            registryCredentialsId "${G_docker_creds}"
                            additionalBuildArgs "--pull --target ton-labs-vm-rust " + 
                                                "--build-arg \"TON_LABS_TYPES_IMAGE=${G_images['ton-labs-types']}\" " +
                                                "--build-arg \"TON_LABS_VM_IMAGE=${G_images['ton-labs-vm']}\""
                        }
                    }
                    steps {
                        script {
                            sh """
                                cd /tonlabs/ton-labs-vm
                                cargo update
                                cargo test --release
                            """
                        }
                    }
                }
            }
        }
    }
    post {
        always {
            node('master') {
                script {
                    cleanWs notFailBuild: true
                }
            } 
        }
    }
}