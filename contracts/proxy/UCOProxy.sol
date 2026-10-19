// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * @title UCOProxy
 * @notice Minimal admin-controlled upgradeable proxy.
 *
 * Storage slots deliberately use ERC-1967-compatible locations so that
 * proxy metadata does not collide with normal Solidity implementation
 * storage layouts.
 *
 * Upgrade flow:
 *   1. Caller must be admin.
 *   2. New implementation must contain contract code.
 *   3. extcodehash(newImplementation) must equal expectedCodeHash.
 *   4. Implementation slot is updated.
 *   5. Optional initialization calldata is delegatecalled against the
 *      new implementation.
 */
contract UCOProxy {
    // keccak256("eip1967.proxy.implementation") - 1
    bytes32 internal constant IMPLEMENTATION_SLOT =
        0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc;

    // keccak256("eip1967.proxy.admin") - 1
    bytes32 internal constant ADMIN_SLOT =
        0xb53127684a568b3173ae13b9f8a6016e243e63b6e8ee1178d6a717850b5d6103;

    error NotAdmin();
    error InvalidImplementation();
    error InvalidImplementationHash();
    error InitializationFailed();
    error AdminCannotBeZero();

    event Upgraded(
        address indexed implementation,
        bytes32 indexed implementationHash
    );

    event AdminChanged(
        address indexed previousAdmin,
        address indexed newAdmin
    );

    modifier onlyAdmin() {
        if (msg.sender != _getAdmin()) {
            revert NotAdmin();
        }
        _;
    }

    constructor(
        address implementation_,
        bytes32 expectedImplementationHash_,
        bytes memory initializationData_
    ) payable {
        if (implementation_ == address(0)) {
            revert InvalidImplementation();
        }

        if (implementation_.code.length == 0) {
            revert InvalidImplementation();
        }

        bytes32 actualHash = implementation_.codehash;

        if (actualHash != expectedImplementationHash_) {
            revert InvalidImplementationHash();
        }

        _setAdmin(msg.sender);
        _setImplementation(implementation_);

        emit Upgraded(implementation_, actualHash);

        if (initializationData_.length != 0) {
            (bool success, bytes memory returndata) =
                implementation_.delegatecall(initializationData_);

            if (!success) {
                _revertWithData(returndata);
            }
        }
    }

    /**
     * @notice Upgrade the implementation after verifying its code hash.
     */
    function upgradeTo(
        address newImplementation,
        bytes32 expectedImplementationHash
    ) external onlyAdmin {
        _upgradeTo(newImplementation, expectedImplementationHash);
    }

    /**
     * @notice Upgrade implementation and initialize it atomically.
     */
    function upgradeToAndCall(
        address newImplementation,
        bytes32 expectedImplementationHash,
        bytes calldata initializationData
    ) external payable onlyAdmin {
        _upgradeTo(newImplementation, expectedImplementationHash);

        if (initializationData.length != 0) {
            (bool success, bytes memory returndata) =
                newImplementation.delegatecall(initializationData);

            if (!success) {
                _revertWithData(returndata);
            }
        }
    }

    /**
     * @notice Change the account allowed to upgrade the proxy.
     */
    function changeAdmin(address newAdmin) external onlyAdmin {
        if (newAdmin == address(0)) {
            revert AdminCannotBeZero();
        }

        address previousAdmin = _getAdmin();

        _setAdmin(newAdmin);

        emit AdminChanged(previousAdmin, newAdmin);
    }

    function admin() external view returns (address) {
        return _getAdmin();
    }

    function implementation() external view returns (address) {
        return _getImplementation();
    }

    function implementationHash() external view returns (bytes32) {
        address impl = _getImplementation();

        if (impl == address(0)) {
            return bytes32(0);
        }

        return impl.codehash;
    }

    function _upgradeTo(
        address newImplementation,
        bytes32 expectedImplementationHash
    ) internal {
        if (newImplementation == address(0)) {
            revert InvalidImplementation();
        }

        if (newImplementation.code.length == 0) {
            revert InvalidImplementation();
        }

        bytes32 actualHash = newImplementation.codehash;

        if (actualHash != expectedImplementationHash) {
            revert InvalidImplementationHash();
        }

        _setImplementation(newImplementation);

        emit Upgraded(newImplementation, actualHash);
    }

    function _getImplementation() internal view returns (address impl) {
        bytes32 slot = IMPLEMENTATION_SLOT;

        assembly {
            impl := sload(slot)
        }
    }

    function _setImplementation(address impl) internal {
        bytes32 slot = IMPLEMENTATION_SLOT;

        assembly {
            sstore(slot, impl)
        }
    }

    function _getAdmin() internal view returns (address adm) {
        bytes32 slot = ADMIN_SLOT;

        assembly {
            adm := sload(slot)
        }
    }

    function _setAdmin(address newAdmin) internal {
        bytes32 slot = ADMIN_SLOT;

        assembly {
            sstore(slot, newAdmin)
        }
    }

    fallback() external payable {
        _delegate(_getImplementation());
    }

    receive() external payable {
        _delegate(_getImplementation());
    }

    function _delegate(address implementation_) internal {
        assembly {
            calldatacopy(0, 0, calldatasize())

            let result := delegatecall(
                gas(),
                implementation_,
                0,
                calldatasize(),
                0,
                0
            )

            returndatacopy(0, 0, returndatasize())

            switch result
            case 0 {
                revert(0, returndatasize())
            }
            default {
                return(0, returndatasize())
            }
        }
    }

    function _revertWithData(bytes memory returndata) internal pure {
        if (returndata.length == 0) {
            revert InitializationFailed();
        }

        assembly {
            revert(add(returndata, 32), mload(returndata))
        }
    }
}
