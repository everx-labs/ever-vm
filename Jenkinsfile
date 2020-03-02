G_giturl = ""
G_gitcred = 'TonJenSSH'
G_docker_creds = "TonJenDockerHub"
G_image_base = "rust:1.40"
G_image_target = ""
G_docker_image = null
G_build = "none"
G_test = "none"
G_binversion = "NotSet"

pipeline {
    tools {nodejs "Node12.8.0"}
    options {
        buildDiscarder logRotator(artifactDaysToKeepStr: '', artifactNumToKeepStr: '', daysToKeepStr: '', numToKeepStr: '1')
        disableConcurrentBuilds()
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
            name:'dockerImage_ton_labs_types',
            defaultValue: 'tonlabs/ton-labs-types:latest',
            description: 'Existing ton-labs-types image name'
        )
        string(
            name:'dockerImage_ton_labs_block',
            defaultValue: 'tonlabs/ton-labs-block:latest',
            description: 'Existing ton-labs-block image name'
        )
        string(
            name:'dockerImage_ton_labs_vm',
            defaultValue: '',
            description: 'Expected ton-labs-vm image name'
        )
        string(
            name:'ton_labs_abi_branch',
            defaultValue: 'master',
            description: 'ton-labs-abi branch for upstairs test'
        )
        string(
            name:'ton_executor_branch',
            defaultValue: 'master',
            description: 'ton-executor branch for upstairs test'
        )
        string(
            name:'tvm_linker_branch',
            defaultValue: 'master',
            description: 'tvm-linker branch for upstairs test'
        )
        string(
            name:'ton_sdk_branch',
            defaultValue: 'master',
            description: 'ton-sdk branch for upstairs test'
        )
    }
    stages {
        stage('Versioning') {
            steps {
                script {
                    withAWS(credentials: 'CI_bucket_writer', region: 'eu-central-1') {
                        identity = awsIdentity()
                        s3Download bucket: 'sdkbinaries.tonlabs.io', file: 'version.json', force: true, path: 'version.json'
                    }
                    if(params.common_version) {
                        G_binversion = sh (script: "node tonVersion.js --set ${params.common_version} .", returnStdout: true).trim()
                    } else {
                        G_binversion = sh (script: "node tonVersion.js .", returnStdout: true).trim()
                    }


                    withAWS(credentials: 'CI_bucket_writer', region: 'eu-central-1') {
                        identity = awsIdentity()
                        s3Upload \
                            bucket: 'sdkbinaries.tonlabs.io', \
                            includePathPattern:'version.json', path: '', \
                            workingDir:'.'
                    }
                }
            }
        }
        stage('Collect commit data') {
            steps {
                sshagent([G_gitcred]) {
                    script {
                        G_giturl = env.GIT_URL
                        echo "${G_giturl}"
                        C_PROJECT = env.GIT_URL.substring(19, env.GIT_URL.length() - 4)
                        C_COMMITER = sh (script: 'git show -s --format=%cn ${GIT_COMMIT}', returnStdout: true).trim()
                        C_TEXT = sh (script: 'git show -s --format=%s ${GIT_COMMIT}', returnStdout: true).trim()
                        C_AUTHOR = sh (script: 'git show -s --format=%an ${GIT_COMMIT}', returnStdout: true).trim()
                        C_HASH = sh (script: 'git show -s --format=%h ${GIT_COMMIT}', returnStdout: true).trim()
                    
                        DiscordURL = "https://discordapp.com/api/webhooks/496992026932543489/4exQIw18D4U_4T0H76bS3Voui4SyD7yCQzLP9IRQHKpwGRJK1-IFnyZLyYzDmcBKFTJw"
                        string DiscordFooter = "Build duration is ${currentBuild.durationString}"
                        DiscordTitle = "Job ${JOB_NAME} from GitHub ${C_PROJECT}"
                        
                        if (params.dockerImage_ton_labs_vm == '') {
                            G_image_target = "tonlabs/ton-labs-vm:${GIT_COMMIT}"
                        } else {
                            G_image_target = params.dockerImage_ton_labs_vm
                        }
                        echo "Target image name: ${G_image_target}"

                        def buildCause = currentBuild.getBuildCauses()
                        echo "Build cause: ${buildCause}"
                    }
                }
            }
        }
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
                            G_image_target, 
                            args
                        )
                        echo "Image ${G_docker_image} as ${G_image_target}"
                    }
                }
            }
        }
        stage('Build') {
            agent {
                dockerfile {
                    registryCredentialsId "${G_docker_creds}"
                    additionalBuildArgs "--pull --target ton-labs-vm-rust " + 
                                        "--build-arg \"TON_LABS_TYPES_IMAGE=${params.dockerImage_ton_labs_types}\" " +
                                        "--build-arg \"TON_LABS_VM_IMAGE=${G_image_target}\""
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
            post {
                success { script { G_build = "success" } }
                failure { script { G_build = "failure" } }
            }
        }
        stage('Tests') {
            agent {
                dockerfile {
                    registryCredentialsId "${G_docker_creds}"
                    additionalBuildArgs "--pull --target ton-labs-vm-rust " + 
                                        "--build-arg \"TON_LABS_TYPES_IMAGE=${params.dockerImage_ton_labs_types}\" " +
                                        "--build-arg \"TON_LABS_VM_IMAGE=${G_image_target}\""
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
            post {
                success { script { G_test = "success" } }
                failure { script { G_test = "failure" } }
            }
        }
        stage('Build ton-executor/ton-labs-abi') {
            when {
                expression {
                    def cause = "${currentBuild.getBuildCauses()}"
                    echo "${cause}"
                    echo "${cause.matches('(.*)ton-labs-types(.*)')}"
                    return !cause.matches("(.*)ton-labs-types(.*)")
                }
            }
            parallel {
                stage('ton-executor') {
                    steps {
                        script {
                            def params_executor = [
                                [
                                    $class: 'StringParameterValue',
                                    name: 'dockerImage_ton_labs_types',
                                    value: "${params.dockerImage_ton_labs_types}"
                                ],
                                [
                                    $class: 'StringParameterValue',
                                    name: 'dockerImage_ton_labs_block',
                                    value: "${params.dockerImage_ton_labs_block}"
                                ],
                                [
                                    $class: 'StringParameterValue',
                                    name: 'dockerImage_ton_labs_vm',
                                    value: "${G_image_target}"
                                ],
                                [
                                    $class: 'StringParameterValue',
                                    name: 'ton_labs_abi_branch',
                                    value: params.ton_labs_abi_branch
                                ],
                                [
                                    $class: 'StringParameterValue',
                                    name: 'ton_executor_branch',
                                    value: params.ton_executor_branch
                                ],
                                [
                                    $class: 'StringParameterValue',
                                    name: 'tvm_linker_branch',
                                    value: params.tvm_linker_branch
                                ],
                                [
                                    $class: 'StringParameterValue',
                                    name: 'ton_sdk_branch',
                                    value: params.ton_sdk_branch
                                ]
                            ]
                            build job: "Node/ton-executor/${params.ton_executor_branch}", parameters: params_executor
                        }
                    }
                    post {
                        success { script { G_test = "success" } }
                        failure { script { G_test = "failure" } }
                    }
                }
                stage('ton-labs-abi') {
                    steps {
                        script {
                            def params_abi = [
                                [
                                    $class: 'StringParameterValue',
                                    name: 'dockerImage_ton_labs_types',
                                    value: "${params.dockerImage_ton_labs_types}"
                                ],
                                [
                                    $class: 'StringParameterValue',
                                    name: 'dockerImage_ton_labs_block',
                                    value: "${params.dockerImage_ton_labs_block}"
                                ],
                                [
                                    $class: 'StringParameterValue',
                                    name: 'dockerImage_ton_labs_vm',
                                    value: "${G_image_target}"
                                ],
                                [
                                    $class: 'StringParameterValue',
                                    name: 'ton_labs_abi_branch',
                                    value: params.ton_labs_abi_branch
                                ],
                                [
                                    $class: 'StringParameterValue',
                                    name: 'ton_executor_branch',
                                    value: params.ton_executor_branch
                                ],
                                [
                                    $class: 'StringParameterValue',
                                    name: 'tvm_linker_branch',
                                    value: params.tvm_linker_branch
                                ],
                                [
                                    $class: 'StringParameterValue',
                                    name: 'ton_sdk_branch',
                                    value: params.ton_sdk_branch
                                ]
                            ]
                            build job: "Node/ton-labs-abi/${params.ton_labs_abi_branch}", parameters: params_abi
                        }
                    }
                    post {
                        success { script { G_test = "success" } }
                        failure { script { G_test = "failure" } }
                    }
                }
            }
        }
        stage('Tag as latest') {
            steps {
                script {
                    docker.withRegistry('', G_docker_creds) {
                        G_docker_image.push('latest')
                    }
                }
            }
        }
    }
    post {
        always {
            node('master') {
                script {
                    DiscordDescription = """${C_COMMITER} pushed commit ${C_HASH} by ${C_AUTHOR} with a message '${C_TEXT}'
Build number ${BUILD_NUMBER}
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
                    cleanWs notFailBuild: true
                }
            } 
        }
        success {
            script {
                def cause = "${currentBuild.getBuildCauses()}"
                echo "${cause}"
                if(!cause.matches('upstream')) {
                    withAWS(credentials: 'CI_bucket_writer', region: 'eu-central-1') {
                        identity = awsIdentity()
                        s3Download bucket: 'sdkbinaries.tonlabs.io', file: 'version.json', force: true, path: 'version.json'
                    }
                    sh """
                        echo const fs = require\\(\\'fs\\'\\)\\; > release.js
                        echo const ver = JSON.parse\\(fs.readFileSync\\(\\'version.json\\'\\, \\'utf8\\'\\)\\)\\; >> release.js
                        echo if\\(!ver.release\\) { throw new Error\\(\\'Empty release field\\'\\); } >> release.js
                        echo if\\(ver.candidate\\) { ver.release = ver.candidate\\; ver.candidate = \\'\\'\\; } >> release.js
                        echo fs.writeFileSync\\(\\'version.json\\', JSON.stringify\\(ver\\)\\)\\; >> release.js
                        cat release.js
                        cat version.json
                        node release.js
                    """
                    withAWS(credentials: 'CI_bucket_writer', region: 'eu-central-1') {
                        identity = awsIdentity()
                        s3Upload \
                            bucket: 'sdkbinaries.tonlabs.io', \
                            includePathPattern:'version.json', workingDir:'.'
                    }
                }
            }
        }
        failure {
            script {
                def cause = "${currentBuild.getBuildCauses()}"
                echo "${cause}"
                if(!cause.matches('upstream')) {
                    withAWS(credentials: 'CI_bucket_writer', region: 'eu-central-1') {
                        identity = awsIdentity()
                        s3Download bucket: 'sdkbinaries.tonlabs.io', file: 'version.json', force: true, path: 'version.json'
                    }
                    sh """
                        echo const fs = require\\(\\'fs\\'\\)\\; > decline.js
                        echo const ver = JSON.parse\\(fs.readFileSync\\(\\'version.json\\'\\, \\'utf8\\'\\)\\)\\; >> decline.js
                        echo if\\(!ver.release\\) { throw new Error\\(\\'Unable to set decline version\\'\\)\\; } >> decline.js
                        echo ver.candidate = \\'\\'\\; >> decline.js
                        echo fs.writeFileSync\\(\\'version.json\\', JSON.stringify\\(ver\\)\\)\\; >> decline.js
                        cat decline.js
                        cat version.json
                        node decline.js
                    """
                    withAWS(credentials: 'CI_bucket_writer', region: 'eu-central-1') {
                        identity = awsIdentity()
                        s3Upload \
                            bucket: 'sdkbinaries.tonlabs.io', \
                            includePathPattern:'version.json', workingDir:'.'
                    }
                }
            }
        }
    }
}
