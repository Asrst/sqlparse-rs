CREATE TABLE `order items` (
  `id` INT AUTO_INCREMENT,
  `qty` DECIMAL(10,2) DEFAULT 0.00,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
SELECT `o`.`id` FROM `order items` `o` WHERE `o`.`qty` > 1.5 LIMIT 10;
