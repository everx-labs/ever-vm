G_giturl = ""
G_gitcred = 'TonJenSSH'
G_docker_creds = "TonJenDockerHub"
G_images = [:]
G_branches = [:]
G_params = null
G_docker_image = null
G_build = "none"
G_test = "none"
G_commit = ""
G_binversion = "NotSet"

def isUpstream() {
    return currentBuild.getBuildCauses()[0]._class.toString() == 'hudson.model.Cause$UpstreamCause'
}

def buildImagesMap() {
    if (params.image_ton_types == '') {
        G_images.put('ton-types', "tonlabs/ton-types:ton-labs-vm-${GIT_COMMIT}")
    } else {
        G_images.put('ton-types', params.image_ton_types)
    }

    if (params.image_ton_labs_types == '') {
        G_images.put('ton-labs-types', "tonlabs/ton-labs-types:ton-labs-vm-${GIT_COMMIT}")
    } else {
        G_images.put('ton-labs-types', params.image_ton_labs_types)
    }

    if (params.image_ton_block == '') {
        G_images.put('ton-block', "tonlabs/ton-block:ton-labs-vm-${GIT_COMMIT}")
    } else {
        G_images.put('ton-block', params.image_ton_block)
    }

    if (params.image_ton_labs_block == '') {
        G_images.put('ton-labs-block', "tonlabs/ton-labs-block:ton-labs-vm-${GIT_COMMIT}")
    } else {
        G_images.put('ton-labs-block', params.image_ton_labs_block)
    }

    if (params.image_ton_vm == '') {
        G_images.put('ton-vm', "tonlabs/ton-vm:ton-labs-vm-${GIT_COMMIT}")
    } else {
        G_images.put('ton-vm', params.image_ton_vm)
    }

    if (params.image_ton_labs_vm == '') {
        G_images.put('ton-labs-vm', "tonlabs/ton-labs-vm:${GIT_COMMIT}")
    } else {
        G_images.put('ton-labs-vm', params.image_ton_labs_vm)
    }

    if (params.image_ton_labs_abi == '') {
        G_images.put('ton-labs-abi', "tonlabs/ton-labs-abi:ton-labs-vm-${GIT_COMMIT}")
    } else {
        G_images.put('ton-labs-abi', params.image_ton_labs_abi)
    }

    if (params.image_ton_executor == '') {
        G_images.put('ton-executor', "tonlabs/ton-executor:ton-labs-vm-${GIT_COMMIT}")
    } else {
        G_images.put('ton-executor', params.image_ton_executor)
    }

    if (params.image_ton_sdk == '') {
        G_images.put('ton-sdk', "tonlabs/ton-sdk:ton-labs-vm-${GIT_COMMIT}")
    } else {
        G_images.put('ton-sdk', params.image_ton_sdk)
    }

    if (params.image_tvm_linker == '') {
        G_images.put('tvm-linker', "tonlabs/tvm_linker:ton-labs-vm-${GIT_COMMIT}")
    } else {
        G_images.put('tvm-linker', params.image_tvm_linker)
    }
}

def buildBranchesMap() {
    if (params.branch_ton_types == '') {
        G_branches.put('ton-types', "master")
    } else {
        G_branches.put('ton-types', params.branch_ton_types)
    }
    
    if (params.branch_ton_labs_types == '') {
        G_branches.put('ton-labs-types', "release-candidate")
    } else {
        G_branches.put('ton-labs-types', params.branch_ton_labs_types)
    }

    if (params.branch_ton_block == '') {
        G_branches.put('ton-block', "master")
    } else {
        G_branches.put('ton-block', params.branch_ton_block)
    }

    if (params.branch_ton_labs_block == '') {
        G_branches.put('ton-labs-block', "release-candidate"
    } else {
        G_branches.put('ton-labs-block', params.branch_ton_labs_block)
    }

    if (params.branch_ton_vm == '') {
        G_branches.put('ton-vm', "master")
    } else {
        G_branches.put('ton-vm', params.branch_ton_vm)
    }

    if (params.branch_ton_labs_vm == '') {
        G_branches.put('ton-labs-vm', "${env.BRANCH_NAME}")
    } else {
        G_branches.put('ton-labs-vm', params.branch_ton_labs_vm)
    }

    if (params.branch_ton_labs_abi == '') {
        G_branches.put('ton-labs-abi', "master")
    } else {
        G_branches.put('ton-labs-abi', params.branch_ton_labs_abi)
    }

    if (params.branch_ton_executor == '') {
        G_branches.put('ton-executor', "master")
    } else {
        G_branches.put('ton-executor', params.branch_ton_executor)
    }

    if (params.branch_ton_sdk == '') {
        G_branches.put('ton-sdk', "master")
    } else {
        G_branches.put('ton-sdk', params.branch_ton_sdk)
    }

    if (params.branch_tvm_linker == '') {
        G_branches.put('tvm-linker', "master")
    } else {
        G_branches.put('tvm-linker', params.branch_tvm_linker)
    }

    if (params.branch_sol2tvm == '') {
        G_branches.put('sol2tvm', "master")
    } else {
        G_branches.put('sol2tvm', params.branch_sol2tvm)
    }
}

def buildParams() {
    buildImagesMap()
    buildBranchesMap()
    G_params = []
    params.each { key, value ->
        def item = null
        def nKey = key.toLowerCase().replaceAll('branch_', '').replaceAll('image_', '').replaceAll('_','-')
        if(key ==~ '^branch_(.)+') {
            item = string("name": key, "value": G_branches["${nKey}"])
        } else {
            if(key ==~ '^image_(.)+') {
                item = string("name": key, "value": G_images["${nKey}"])
            } else {
                if(key == 'common_version') {
                    item = string("name": key, "value": G_binversion)
                } else {
                    item = string("name": key, "value": value)
                }
            }
        }
        G_params.push(item)
    }
}

pipeline {
    tools {nodejs "Node12.8.0"}
    options {
        buildDiscarder logRotator(artifactDaysToKeepStr: '', artifactNumToKeepStr: '', daysToKeepStr: '', numToKeepStr: '1')
        
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
            name:'branch_ton_types',
            defaultValue: 'master',
            description: 'ton-types branch for dependency test'
        )
        string(
            name:'image_ton_types',
            defaultValue: '',
            description: 'ton-types image name'
        )
        string(
            name:'branch_ton_labs_types',
            defaultValue: '',
            description: 'ton-labs-types branch for dependency test'
        )
        string(
            name:'image_ton_labs_types',
            defaultValue: '',
            description: 'ton-labs-types image name'
        )
        string(
            name:'branch_ton_block',
            defaultValue: 'master',
            description: 'ton-block branch'
        )
        string(
            name:'image_ton_block',
            defaultValue: '',
            description: 'ton-block image name'
        )
        string(
            name:'branch_ton_labs_block',
            defaultValue: '',
            description: 'ton-labs-block branch'
        )
        string(
            name:'image_ton_labs_block',
            defaultValue: '',
            description: 'ton-labs-block image name'
        )
        string(
            name:'branch_ton_vm',
            defaultValue: 'master',
            description: 'ton-vm branch'
        )
        string(
            name:'image_ton_vm',
            defaultValue: '',
            description: 'ton-vm image name'
        )
        string(
            name:'branch_ton_labs_vm',
            defaultValue: '',
            description: 'ton-labs-vm branch'
        )
        string(
            name:'image_ton_labs_vm',
            defaultValue: '',
            description: 'ton-labs-vm image name'
        )
        string(
            name:'branch_ton_labs_abi',
            defaultValue: 'master',
            description: 'ton-labs-abi branch'
        )
        string(
            name:'image_ton_labs_abi',
            defaultValue: '',
            description: 'ton-labs-abi image name'
        )
        string(
            name:'branch_ton_executor',
            defaultValue: 'master',
            description: 'ton-executor branch'
        )
        string(
            name:'image_ton_executor',
            defaultValue: '',
            description: 'ton-executor image name'
        )
        string(
            name:'branch_tvm_linker',
            defaultValue: 'master',
            description: 'tvm-linker branch'
        )
        string(
            name:'image_tvm_linker',
            defaultValue: '',
            description: 'tvm-linker image name'
        )
        string(
            name:'branch_ton_sdk',
            defaultValue: 'master',
            description: 'ton-sdk branch'
        )
        string(
            name:'image_ton_sdk',
            defaultValue: '',
            description: 'ton-sdk image name'
        )
        string(
            name:'branch_sol2tvm',
            defaultValue: 'master',
            description: 'sol2tvm branch'
        )
    }
    stages {
        stage('Versioning') {
            steps {
                script {
                    withAWS(credentials: 'CI_bucket_writer', region: 'eu-central-1') {
                        identity = awsIdentity()
                        s3Download bucket: 'sdkbinaries-ws.tonlabs.io', file: 'version.json', force: true, path: 'version.json'
                    }
                    if(params.common_version) {
                        G_binversion = sh (script: "node tonVersion.js --set ${params.common_version} .", returnStdout: true).trim()
                    } else {
                        G_binversion = sh (script: "node tonVersion.js .", returnStdout: true).trim()
                    }


                    withAWS(credentials: 'CI_bucket_writer', region: 'eu-central-1') {
                        identity = awsIdentity()
                        s3Upload \
                            bucket: 'sdkbinaries-ws.tonlabs.io', \
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
                        sh "git fetch && git pull"
                        G_giturl = env.GIT_URL
                        G_commit = GIT_COMMIT
                        echo "${G_giturl}"
                        C_PROJECT = env.GIT_URL.substring(19, env.GIT_URL.length() - 4)
                        C_COMMITER = sh (script: 'git show -s --format=%cn ${GIT_COMMIT}', returnStdout: true).trim()
                        C_TEXT = sh (script: 'git show -s --format=%s ${GIT_COMMIT}', returnStdout: true).trim()
                        C_AUTHOR = sh (script: 'git show -s --format=%an ${GIT_COMMIT}', returnStdout: true).trim()
                        C_HASH = sh (script: 'git show -s --format=%h ${GIT_COMMIT}', returnStdout: true).trim()
                    
                        DiscordURL = "https://discordapp.com/api/webhooks/496992026932543489/4exQIw18D4U_4T0H76bS3Voui4SyD7yCQzLP9IRQHKpwGRJK1-IFnyZLyYzDmcBKFTJw"
                        string DiscordFooter = "Build duration is ${currentBuild.durationString}"
                        DiscordTitle = "Job ${JOB_NAME} from GitHub ${C_PROJECT}"
                        
                        def buildCause = currentBuild.getBuildCauses()[0].shortDescription
                        echo "Build cause: ${buildCause}"
                        
                        buildParams()
                        echo "${G_params}"
                    }
                }
            }
        }
        stage('Before stages') {
            when {
                expression {
                    return !isUpstream()
                }
            }
            steps {
                script {
                    def beforeParams = G_params
                    beforeParams.push(string("name": "project_name", "value": "ton-labs-vm"))
                    beforeParams.push(string("name": "stage", "value": "before"))
                    build job: 'Builder/build-flow', parameters: beforeParams
                }
            }
        }
        stage('Build stages') {
            parallel {
                stage('Parallel stages') {
                    when {
                        expression {
                            return !isUpstream()
                        }
                    }
                    steps {
                        script {
                            def intimeParams = G_params
                            intimeParams.push(string("name": "project_name", "value": "ton-labs-vm"))
                            intimeParams.push(string("name": "stage", "value": "in_time"))
                            build job: 'Builder/build-flow', parameters: intimeParams
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
                            post {
                                success { script { G_test = "success" } }
                                failure { script { G_test = "failure" } }
                            }
                        }
                    }
                }
            }
        }
        stage('After stages') {
            when {
                expression {
                    return !isUpstream()
                }
            }
            steps {
                script {
                    def afterParams = G_params
                    afterParams.push(string("name": "project_name", "value": "ton-labs-vm"))
                    afterParams.push(string("name": "stage", "value": "after"))
                    build job: 'Builder/build-flow', parameters: afterParams
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
        failure {
            script {
                def cause = "${currentBuild.getBuildCauses()}"
                echo "${cause}"
                if(!cause.matches('upstream')) {
                    withAWS(credentials: 'CI_bucket_writer', region: 'eu-central-1') {
                        identity = awsIdentity()
                        s3Download bucket: 'sdkbinaries-ws.tonlabs.io', file: 'version.json', force: true, path: 'version.json'
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
                            bucket: 'sdkbinaries-ws.tonlabs.io', \
                            includePathPattern:'version.json', workingDir:'.'
                    }
                }
            }
        }
    }
}
